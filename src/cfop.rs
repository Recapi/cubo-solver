//! Solver "humano" por etapas (CFOP): cruz -> F2L -> OLL -> PLL.
//!
//! O cubo e reorientado para a base/frente escolhidas (a cruz e feita na base,
//! embaixo). Cruz e pares de F2L saem de busca IDA* com tabelas EXATAS dos
//! subconjuntos (cruz: 331.776 estados; cada par: 576) — otimas por etapa.
//! OLL e PLL usam algoritmos reais de speedcubing (so giros externos) testados
//! por simulacao: um algoritmo so e escolhido se, aplicado, resolve a etapa.
//! Se nenhum algoritmo de 1 olhada servir, uma composicao curta de algoritmos
//! basicos (F R U R' U' F', Sune, A-perm, U-perm, T-perm) garante a cobertura.

use std::sync::OnceLock;

use crate::cube::{move_face, CubieCube};
use crate::facelet::{all_rotations, rotate_cube, rotation_perm, to_facelets};
use crate::tables::Tables;

pub struct CfopStage {
    pub name: String,
    pub info: String,
    pub moves: Vec<u8>,
}

pub struct CfopSolution {
    /// Planificacao inicial no referencial de resolucao (base embaixo).
    pub start_facelets: String,
    pub stages: Vec<CfopStage>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Tabelas exatas da cruz e dos pares (construidas uma vez)
// ---------------------------------------------------------------------------

struct CfopTables {
    /// Distancia exata da cruz: 4 arestas da base (24^4 posicoes codificadas).
    cross: Vec<u8>,
    /// Distancia exata de cada par F2L (canto 4+k, aresta 8+k): 24 x 24.
    pairs: [Vec<u8>; 4],
}

static CFOP: OnceLock<CfopTables> = OnceLock::new();

fn cfop_tables(t: &Tables) -> &'static CfopTables {
    CFOP.get_or_init(|| {
        let mut edge_dst = [[(0u8, 0u8); 12]; 18];
        let mut corner_dst = [[(0u8, 0u8); 8]; 18];
        for m in 0..18 {
            let mv = &t.mc[m];
            for s in 0..12 {
                let s2 = mv.ep.iter().position(|&x| x == s as u8).unwrap();
                edge_dst[m][s] = (s2 as u8, mv.eo[s2]);
            }
            for s in 0..8 {
                let s2 = mv.cp.iter().position(|&x| x == s as u8).unwrap();
                corner_dst[m][s] = (s2 as u8, mv.co[s2]);
            }
        }

        // BFS exata da cruz a partir da posicao resolvida.
        let mut cross = vec![255u8; 24 * 24 * 24 * 24];
        {
            let start = [4u8 * 2, 5 * 2, 6 * 2, 7 * 2]; // DR DF DL DB em casa
            let idx = |v: &[u8; 4]| {
                ((v[0] as usize * 24 + v[1] as usize) * 24 + v[2] as usize) * 24 + v[3] as usize
            };
            cross[idx(&start)] = 0;
            let mut frontier = vec![start];
            let mut d = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::with_capacity(frontier.len() * 4);
                for st in &frontier {
                    for m in 0..18 {
                        let mut v = [0u8; 4];
                        for k in 0..4 {
                            let (s2, fl) = edge_dst[m][(st[k] / 2) as usize];
                            v[k] = s2 * 2 + ((st[k] & 1) ^ fl);
                        }
                        let i = idx(&v);
                        if cross[i] == 255 {
                            cross[i] = d + 1;
                            next.push(v);
                        }
                    }
                }
                frontier = next;
                d += 1;
            }
        }

        // BFS exata de cada par F2L.
        let pairs = std::array::from_fn(|k| {
            let mut tab = vec![255u8; 24 * 24];
            let start = ((4 + k) * 3 * 24 + (8 + k) * 2) as usize;
            tab[start] = 0;
            let mut frontier = vec![((4 + k) as u8 * 3, (8 + k) as u8 * 2)];
            let mut d = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &(cv, ev) in &frontier {
                    for m in 0..18 {
                        let (cs, tw) = corner_dst[m][(cv / 3) as usize];
                        let c2 = cs * 3 + (cv % 3 + tw) % 3;
                        let (es, fl) = edge_dst[m][(ev / 2) as usize];
                        let e2 = es * 2 + ((ev & 1) ^ fl);
                        let i = c2 as usize * 24 + e2 as usize;
                        if tab[i] == 255 {
                            tab[i] = d + 1;
                            next.push((c2, e2));
                        }
                    }
                }
                frontier = next;
                d += 1;
            }
            tab
        });

        let _ = (edge_dst, corner_dst);
        CfopTables { cross, pairs }
    })
}

// ---------------------------------------------------------------------------
// Indices dos subconjuntos num estado completo
// ---------------------------------------------------------------------------

fn cross_index(c: &CubieCube) -> usize {
    let mut v = [0usize; 4];
    for i in 0..12 {
        let e = c.ep[i] as usize;
        if (4..8).contains(&e) {
            v[e - 4] = i * 2 + c.eo[i] as usize;
        }
    }
    ((v[0] * 24 + v[1]) * 24 + v[2]) * 24 + v[3]
}

fn pair_index(c: &CubieCube, k: usize) -> usize {
    let mut cv = 0;
    let mut ev = 0;
    for i in 0..8 {
        if c.cp[i] as usize == 4 + k {
            cv = i * 3 + c.co[i] as usize;
        }
    }
    for i in 0..12 {
        if c.ep[i] as usize == 8 + k {
            ev = i * 2 + c.eo[i] as usize;
        }
    }
    cv * 24 + ev
}

// ---------------------------------------------------------------------------
// Buscas IDA* das etapas de construcao (cruz e pares)
// ---------------------------------------------------------------------------

struct StageSearch<'a> {
    t: &'a Tables,
    ct: &'a CfopTables,
    /// pares que precisam estar resolvidos no final (alem da cruz)
    need: Vec<usize>,
    path: [u8; 16],
    found: Option<Vec<u8>>,
}

impl<'a> StageSearch<'a> {
    fn h(&self, c: &CubieCube) -> u8 {
        let mut h = self.ct.cross[cross_index(c)];
        for &k in &self.need {
            h = h.max(self.ct.pairs[k][pair_index(c, k)]);
        }
        h
    }

    fn dfs(&mut self, c: &CubieCube, depth: usize, n: usize) -> bool {
        let h = self.h(c) as usize;
        if h > depth {
            return false;
        }
        if depth == 0 {
            self.found = Some(self.path[..n].to_vec());
            return true;
        }
        for m in 0..18u8 {
            if n > 0 {
                let lf = move_face(self.path[n - 1]);
                let f = move_face(m);
                if f == lf || (lf >= 3 && f + 3 == lf) {
                    continue;
                }
            }
            let d = c.multiply(&self.t.mc[m as usize]);
            self.path[n] = m;
            if self.dfs(&d, depth - 1, n + 1) {
                return true;
            }
        }
        false
    }

    fn run(&mut self, c: &CubieCube, cap: usize) -> Option<Vec<u8>> {
        let h0 = self.h(c) as usize;
        for d in h0..=cap {
            if self.dfs(c, d, 0) {
                return self.found.take();
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Algoritmos de OLL / PLL (apenas giros externos) e casamento por simulacao
// ---------------------------------------------------------------------------

fn parse_alg(s: &str) -> Vec<u8> {
    let names = crate::cube::MOVE_NAMES;
    s.split_whitespace()
        .map(|tok| names.iter().position(|&m| m == tok).expect("alg invalido") as u8)
        .collect()
}

fn apply(t: &Tables, c: &CubieCube, moves: &[u8]) -> CubieCube {
    let mut r = *c;
    for &m in moves {
        r = r.multiply(&t.mc[m as usize]);
    }
    r
}

fn f2l_done(c: &CubieCube) -> bool {
    (4..8).all(|i| c.cp[i] as usize == i && c.co[i] == 0)
        && (4..12).all(|i| c.ep[i] as usize == i && c.eo[i] == 0)
}

fn oll_done(c: &CubieCube) -> bool {
    f2l_done(c) && (0..4).all(|i| c.co[i] == 0 && c.eo[i] == 0)
}

/// U, U2 ou U' como prefixo (0 = nada).
fn auf(k: usize) -> Vec<u8> {
    match k {
        1 => vec![0],
        2 => vec![1],
        3 => vec![2],
        _ => vec![],
    }
}

struct NamedAlg {
    name: &'static str,
    moves: Vec<u8>,
}

fn oll_algs() -> Vec<NamedAlg> {
    [
        ("Sune (OLL 27)", "R U R' U R U2 R'"),
        ("Antisune (OLL 26)", "R U2 R' U' R U' R'"),
        ("H (OLL 21)", "R U2 R' U' R U R' U' R U' R'"),
        ("Pi (OLL 22)", "R U2 R2 U' R2 U' R2 U2 R"),
        ("Headlights (OLL 23)", "R2 D R' U2 R D' R' U2 R'"),
        ("T (OLL 25)", "R' F R B' R' F' R B"),
        ("Linha (OLL 45)", "F R U R' U' F'"),
        ("L pequeno (OLL 44)", "F U R U' R' F'"),
        ("L pequeno (OLL 43)", "F' U' L' U L F"),
        ("OLL 51", "F U R U' R' U R U' R' F'"),
        ("OLL 33", "R U R' U' R' F R F'"),
        ("OLL 48", "F R U R' U' R U R' U' F'"),
    ]
    .into_iter()
    .map(|(n, a)| NamedAlg { name: n, moves: parse_alg(a) })
    .collect()
}

fn pll_algs() -> Vec<NamedAlg> {
    [
        ("T-perm", "R U R' U' R' F R2 U' R' U' R U R' F'"),
        ("Ua-perm", "R U' R U R U R U' R' U' R2"),
        ("Ub-perm", "R2 U R U R' U' R' U' R' U R'"),
        ("Aa-perm", "R' F R' B2 R F' R' B2 R2"),
        ("Ab-perm", "R2 B2 R F R' B2 R F' R"),
        ("Ja-perm", "R' U L' U2 R U' R' U2 R L"),
        ("Jb-perm", "R U R' F' R U R' U' R' F R2 U' R'"),
        ("Y-perm", "F R U' R' U' R U R' F' R U R' U' R' F R F'"),
        ("F-perm", "R' U' F' R U R' U' R' F R2 U' R' U' R U R' U R"),
        ("Ra-perm", "R U' R' U' R U R D R' U' R D' R' U2 R'"),
        ("E-perm", "R B' R' F R B R' F' R B R' F R B' R' F'"),
        ("V-perm", "R' U R' U' B' R' B2 U' B' U B' R B R"),
    ]
    .into_iter()
    .map(|(n, a)| NamedAlg { name: n, moves: parse_alg(a) })
    .collect()
}

/// Melhor algoritmo de 1 olhada: menor (AUF + alg) que conclui a etapa.
fn best_one_look(
    t: &Tables,
    c: &CubieCube,
    algs: &[NamedAlg],
    done: fn(&CubieCube) -> bool,
    with_final_auf: bool,
) -> Option<(String, Vec<u8>)> {
    let mut best: Option<(String, Vec<u8>)> = None;
    for a in algs {
        for k in 0..4 {
            let mut mv = auf(k);
            mv.extend_from_slice(&a.moves);
            let c2 = apply(t, c, &mv);
            let finals = if with_final_auf { 4 } else { 1 };
            for f in 0..finals {
                let mut mv2 = mv.clone();
                mv2.extend(auf(f));
                let c3 = if f == 0 { c2 } else { apply(t, &c2, &auf(f)) };
                if done(&c3) {
                    let melhor = best.as_ref().map(|(_, b)| b.len()).unwrap_or(usize::MAX);
                    if mv2.len() < melhor {
                        best = Some((a.name.to_string(), mv2));
                    }
                }
            }
        }
    }
    best
}

/// Cobertura garantida: composicao de ate `max_blocks` blocos (AUF + alg basico),
/// com AUF final opcional. Aprofundamento iterativo = menos blocos primeiro.
fn fallback_blocks(
    t: &Tables,
    c: &CubieCube,
    basics: &[Vec<u8>],
    done: fn(&CubieCube) -> bool,
    with_final_auf: bool,
    max_blocks: usize,
) -> Option<Vec<u8>> {
    fn rec(
        t: &Tables,
        c: &CubieCube,
        basics: &[Vec<u8>],
        done: fn(&CubieCube) -> bool,
        with_final_auf: bool,
        blocks_left: usize,
        acc: &mut Vec<u8>,
    ) -> bool {
        let finals = if with_final_auf { 4 } else { 1 };
        for f in 0..finals {
            let c2 = if f == 0 { *c } else { apply(t, c, &auf(f)) };
            if done(&c2) {
                acc.extend(auf(f));
                return true;
            }
        }
        if blocks_left == 0 {
            return false;
        }
        for k in 0..4 {
            for alg in basics {
                let mut mv = auf(k);
                mv.extend_from_slice(alg);
                let c2 = apply(t, c, &mv);
                let len0 = acc.len();
                acc.extend_from_slice(&mv);
                if rec(t, &c2, basics, done, with_final_auf, blocks_left - 1, acc) {
                    return true;
                }
                acc.truncate(len0);
            }
        }
        false
    }

    for blocks in 0..=max_blocks {
        let mut acc = Vec::new();
        if rec(t, c, basics, done, with_final_auf, blocks, &mut acc) {
            return Some(acc);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pipeline completo
// ---------------------------------------------------------------------------

const PAIR_NAMES: [&str; 4] = [
    "frente-direita",
    "frente-esquerda",
    "trás-esquerda",
    "trás-direita",
];

/// `base` e `front` sao letras de face (0..6) do esquema do estado: a cor do
/// centro `base` vai para baixo e a do centro `front` para a frente.
pub fn solve_cfop(
    cube: &CubieCube,
    t: &Tables,
    base: usize,
    front: usize,
) -> Result<CfopSolution, String> {
    let rot = all_rotations()
        .into_iter()
        .find(|pi| pi[base] == 3 && pi[front] == 2)
        .ok_or_else(|| {
            "base e frente precisam ser cores vizinhas (nao iguais nem opostas)".to_string()
        })?;
    let start = rotate_cube(cube, &rot, &rotation_perm(&rot));
    let ct = cfop_tables(t);

    let mut stages: Vec<CfopStage> = Vec::new();
    let mut c = start;

    // ---- cruz ----------------------------------------------------------
    let mut s = StageSearch { t, ct, need: vec![], path: [0; 16], found: None };
    let cross_moves = s.run(&c, 9).ok_or("nao achei a cruz (nao deveria acontecer)")?;
    c = apply(t, &c, &cross_moves);
    if !cross_moves.is_empty() {
        stages.push(CfopStage {
            name: "Cruz".into(),
            info: "Forme a cruz na base: as 4 arestas da cor de baixo, alinhadas com os centros."
                .into(),
            moves: cross_moves,
        });
    }

    // ---- F2L: 4 pares, na ordem que der solucoes mais curtas ------------
    let mut solved: Vec<usize> = Vec::new();
    while solved.len() < 4 {
        let mut best: Option<(usize, Vec<u8>)> = None;
        for k in 0..4 {
            if solved.contains(&k) {
                continue;
            }
            let mut need = solved.clone();
            need.push(k);
            let mut s = StageSearch { t, ct, need, path: [0; 16], found: None };
            if let Some(mv) = s.run(&c, 13) {
                if best.as_ref().map(|(_, b)| mv.len() < b.len()).unwrap_or(true) {
                    best = Some((k, mv));
                }
            }
        }
        let (k, mv) = best.ok_or("nao achei um par de F2L (nao deveria acontecer)")?;
        c = apply(t, &c, &mv);
        solved.push(k);
        if !mv.is_empty() {
            stages.push(CfopStage {
                name: format!("F2L {} ({})", solved.len(), PAIR_NAMES[k]),
                info: format!(
                    "Junte o canto e a aresta do par {} e encaixe no lugar.",
                    PAIR_NAMES[k]
                ),
                moves: mv,
            });
        }
    }

    // ---- OLL ------------------------------------------------------------
    if !oll_done(&c) {
        let (name, mv) = match best_one_look(t, &c, &oll_algs(), oll_done, false) {
            Some(v) => v,
            None => {
                let basics = vec![
                    parse_alg("F R U R' U' F'"),
                    parse_alg("R U R' U R U2 R'"),  // Sune
                    parse_alg("R U2 R' U' R U' R'"), // Antisune
                ];
                let mv = fallback_blocks(t, &c, &basics, oll_done, false, 6)
                    .ok_or("OLL sem cobertura (nao deveria acontecer)")?;
                ("OLL em 2 olhadas".to_string(), mv)
            }
        };
        c = apply(t, &c, &mv);
        stages.push(CfopStage {
            name: format!("OLL — {name}"),
            info: "Oriente a última camada: deixe toda a face de cima com a mesma cor.".into(),
            moves: mv,
        });
    }

    // ---- PLL ------------------------------------------------------------
    if !c.is_solved() {
        let done_solved = |x: &CubieCube| x.is_solved();
        let (name, mv) = match best_one_look(t, &c, &pll_algs(), done_solved, true) {
            Some(v) => v,
            None => {
                let basics = vec![
                    parse_alg("R' F R' B2 R F' R' B2 R2"), // Aa
                    parse_alg("R U' R U R U R U' R' U' R2"), // Ua
                    parse_alg("R U R' U' R' F R2 U' R' U' R U R' F'"), // T
                ];
                let mv = fallback_blocks(t, &c, &basics, done_solved, true, 4)
                    .ok_or("PLL sem cobertura (nao deveria acontecer)")?;
                ("PLL em passos".to_string(), mv)
            }
        };
        c = apply(t, &c, &mv);
        let so_auf = mv.iter().all(|&m| move_face(m) == 0);
        stages.push(CfopStage {
            name: if so_auf { "Ajuste final (AUF)".to_string() } else { format!("PLL — {name}") },
            info: if so_auf {
                "Gire a camada de cima até tudo se alinhar.".into()
            } else {
                "Permute as peças da última camada até o cubo fechar.".into()
            },
            moves: mv,
        });
    }

    if !c.is_solved() {
        return Err("erro interno: o CFOP nao fechou o cubo".into());
    }
    let total = stages.iter().map(|s| s.moves.len()).sum();
    Ok(CfopSolution { start_facelets: to_facelets(&start), stages, total })
}
