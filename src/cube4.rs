//! Cubo 4x4 (Rubik's Revenge), resolvido por REDUCAO:
//!
//!   1. centros: agrupar os 4 centros de cada cor na face certa (o esquema de
//!      cores vem do canto DBL, como no 2x2) — busca IDA* com tabelas exatas
//!      por cor (C(24,4) = 10.626 posicoes dos 4 centros de uma cor);
//!   2. arestas: parear as 24 "asas" em 12 pares — busca IDA* com uma tabela
//!      exata da distancia de duas asas ate algum encaixe pareado;
//!   3. resolver como 3x3 (o solver existente), traduzindo para giros externos.
//!
//! As paridades do 4x4 (que tornam o 3x3 reduzido "impossivel") sao corrigidas
//! por algoritmos classicos CERTIFICADOS por simulacao no proprio teste: um
//! candidato so e usado se, aplicado ao cubo montado, preserva centros e pares
//! e alterna exatamente a paridade que promete.
//!
//! Tudo opera no nivel da planificacao: 96 adesivos (6 faces x 16), movimentos
//! como permutacoes geradas por "girar face" + "ciclar 4 tiras".

use std::sync::OnceLock;

use crate::search::{self, SolveParams};
use crate::tables::Tables;

pub const N_FACELETS4: usize = 96;
const FACES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

#[inline]
fn p(face: usize, r: usize, c: usize) -> usize {
    face * 16 + r * 4 + c
}

// ---------------------------------------------------------------------------
// Movimentos: 18 externos + 18 wide (externo + fatia interna), como permutacoes
// ---------------------------------------------------------------------------

pub const N_MOVES4: usize = 36;

/// Nomes na notacao padrao: indices 0..17 = externos, 18..35 = wide (Xw).
pub fn move_name4(m: usize) -> String {
    let faces = ["U", "R", "F", "D", "L", "B"];
    let pow = ["", "2", "'"];
    if m < 18 {
        format!("{}{}", faces[m / 3], pow[m % 3])
    } else {
        let m = m - 18;
        format!("{}w{}", faces[m / 3], pow[m % 3])
    }
}

pub struct Moves4 {
    /// perm[m][src] = destino do adesivo em src.
    pub perm: Vec<[u16; N_FACELETS4]>,
}

static MOVES4: OnceLock<Moves4> = OnceLock::new();

fn identity96() -> [u16; N_FACELETS4] {
    std::array::from_fn(|i| i as u16)
}

fn compose96(a: &[u16; N_FACELETS4], b: &[u16; N_FACELETS4]) -> [u16; N_FACELETS4] {
    // aplicar a, depois b
    let mut r = identity96();
    for s in 0..N_FACELETS4 {
        r[s] = b[a[s] as usize];
    }
    r
}

fn rotate_face_cw(perm: &mut [u16; N_FACELETS4], face: usize) {
    for r in 0..4 {
        for c in 0..4 {
            perm[p(face, r, c)] = p(face, c, 3 - r) as u16;
        }
    }
}

fn cycle_strips(perm: &mut [u16; N_FACELETS4], strips: [[usize; 4]; 4]) {
    for k in 0..4 {
        let from = strips[k];
        let to = strips[(k + 1) % 4];
        for i in 0..4 {
            perm[from[i]] = to[i] as u16;
        }
    }
}

/// Os 12 movimentos base de 90 graus (6 externos + 6 fatias internas).
fn base_moves() -> [
    [u16; N_FACELETS4]; 12
] {
    let (u, rr, f, d, l, b) = (0usize, 1, 2, 3, 4, 5);
    let row = |fc: usize, r: usize| -> [usize; 4] { std::array::from_fn(|i| p(fc, r, i)) };
    let row_rev = |fc: usize, r: usize| -> [usize; 4] { std::array::from_fn(|i| p(fc, r, 3 - i)) };
    let col = |fc: usize, c: usize| -> [usize; 4] { std::array::from_fn(|i| p(fc, i, c)) };
    let col_rev = |fc: usize, c: usize| -> [usize; 4] { std::array::from_fn(|i| p(fc, 3 - i, c)) };

    let mut out = [identity96(); 12];

    // U externo: F linha0 -> L -> B -> R -> F
    rotate_face_cw(&mut out[0], u);
    cycle_strips(&mut out[0], [row(f, 0), row(l, 0), row(b, 0), row(rr, 0)]);
    // fatia u: linha 1
    cycle_strips(&mut out[6], [row(f, 1), row(l, 1), row(b, 1), row(rr, 1)]);

    // R externo: F col3 -> U col3; U col3 -> B col0 (invertida); B -> D; D -> F
    rotate_face_cw(&mut out[1], rr);
    cycle_strips(&mut out[1], [col(f, 3), col(u, 3), col_rev(b, 0), col(d, 3)]);
    // fatia r: col 2 (B col 1)
    cycle_strips(&mut out[7], [col(f, 2), col(u, 2), col_rev(b, 1), col(d, 2)]);

    // F externo: U linha3 -> R col0 -> D linha0 (invertida) -> L col3 (invertida)
    rotate_face_cw(&mut out[2], f);
    cycle_strips(&mut out[2], [row(u, 3), col(rr, 0), row_rev(d, 0), col_rev(l, 3)]);
    // fatia f: U linha2, R col1, D linha1, L col2
    cycle_strips(&mut out[8], [row(u, 2), col(rr, 1), row_rev(d, 1), col_rev(l, 2)]);

    // D externo: F linha3 -> R -> B -> L
    rotate_face_cw(&mut out[3], d);
    cycle_strips(&mut out[3], [row(f, 3), row(rr, 3), row(b, 3), row(l, 3)]);
    // fatia d: linha 2
    cycle_strips(&mut out[9], [row(f, 2), row(rr, 2), row(b, 2), row(l, 2)]);

    // L externo: U col0 -> F col0 -> D col0 -> B col3 (invertida)
    rotate_face_cw(&mut out[4], l);
    cycle_strips(&mut out[4], [col(u, 0), col(f, 0), col(d, 0), col_rev(b, 3)]);
    // fatia l: col 1 (B col 2)
    cycle_strips(&mut out[10], [col(u, 1), col(f, 1), col(d, 1), col_rev(b, 2)]);

    // B externo: U linha0 -> L col0 (invertida) -> D linha3 (invertida) -> R col3
    rotate_face_cw(&mut out[5], b);
    cycle_strips(&mut out[5], [row(u, 0), col_rev(l, 0), row_rev(d, 3), col(rr, 3)]);
    // fatia b: U linha1, L col1, D linha2, R col2
    cycle_strips(&mut out[11], [row(u, 1), col_rev(l, 1), row_rev(d, 2), col(rr, 2)]);

    out
}

pub fn moves4() -> &'static Moves4 {
    MOVES4.get_or_init(|| {
        let base = base_moves();
        let mut perm = Vec::with_capacity(N_MOVES4);
        // 18 externos
        for f in 0..6 {
            let one = base[f];
            let two = compose96(&one, &one);
            let three = compose96(&two, &one);
            perm.push(one);
            perm.push(two);
            perm.push(three);
        }
        // 18 wide = externo + fatia interna
        for f in 0..6 {
            let one = compose96(&base[f], &base[6 + f]);
            let two = compose96(&one, &one);
            let three = compose96(&two, &one);
            perm.push(one);
            perm.push(two);
            perm.push(three);
        }
        Moves4 { perm }
    })
}

pub fn apply_move4(state: &mut [u8; N_FACELETS4], m: usize) {
    let perm = &moves4().perm[m];
    let old = *state;
    for s in 0..N_FACELETS4 {
        state[perm[s] as usize] = old[s];
    }
}

pub fn apply_seq4(state: &mut [u8; N_FACELETS4], seq: &[usize]) {
    for &m in seq {
        apply_move4(state, m);
    }
}

/// Notacao -> indices: externos (R, R2, R'), wide (Rw / r minusculo).
pub fn parse_moves4(s: &str) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for tok in s.split_whitespace() {
        let t = tok.replace('\u{2019}', "'");
        let (body, pow) = if let Some(x) = t.strip_suffix('\'') {
            (x.to_string(), 2usize)
        } else if let Some(x) = t.strip_suffix('2') {
            (x.to_string(), 1)
        } else {
            (t.clone(), 0)
        };
        let (face_str, wide) = if let Some(x) = body.strip_suffix('w') {
            (x.to_uppercase(), true)
        } else if body.len() == 1 && body.chars().next().unwrap().is_lowercase() {
            (body.to_uppercase(), true)
        } else {
            (body.to_uppercase(), false)
        };
        let f = FACES
            .iter()
            .position(|&c| c.to_string() == face_str)
            .ok_or_else(|| format!("movimento desconhecido: \"{tok}\""))?;
        out.push(if wide { 18 + f * 3 + pow } else { f * 3 + pow });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Pecas: cantos, asas (wings) e centros
// ---------------------------------------------------------------------------

/// Adesivos dos 8 cantos (ordem de cores igual ao 3x3: URF, UFL, ...).
pub fn corner_facelets4() -> [[usize; 3]; 8] {
    [
        [p(0, 3, 3), p(1, 0, 0), p(2, 0, 3)], // URF
        [p(0, 3, 0), p(2, 0, 0), p(4, 0, 3)], // UFL
        [p(0, 0, 0), p(4, 0, 0), p(5, 0, 3)], // ULB
        [p(0, 0, 3), p(5, 0, 0), p(1, 0, 3)], // UBR
        [p(3, 0, 3), p(2, 3, 3), p(1, 3, 0)], // DFR
        [p(3, 0, 0), p(4, 3, 3), p(2, 3, 0)], // DLF
        [p(3, 3, 0), p(5, 3, 3), p(4, 3, 0)], // DBL
        [p(3, 3, 3), p(1, 3, 3), p(5, 3, 0)], // DRB
    ]
}

/// As 24 asas: (adesivo primario, adesivo secundario). Duas asas consecutivas
/// (2k, 2k+1) formam o "encaixe" da aresta k do 3x3.
pub fn wing_facelets4() -> [[usize; 2]; 24] {
    let (u, rr, f, d, l, b) = (0usize, 1, 2, 3, 4, 5);
    [
        // UR (U col3 r1/r2 <-> R linha0 c2/c1)
        [p(u, 1, 3), p(rr, 0, 2)],
        [p(u, 2, 3), p(rr, 0, 1)],
        // UF (U linha3 c1/c2 <-> F linha0 c1/c2)
        [p(u, 3, 1), p(f, 0, 1)],
        [p(u, 3, 2), p(f, 0, 2)],
        // UL (U col0 r2/r1 <-> L linha0 c2/c1... L linha0: c0 junto de B)
        [p(u, 1, 0), p(l, 0, 1)],
        [p(u, 2, 0), p(l, 0, 2)],
        // UB (U linha0 c2/c1 <-> B linha0 c1/c2)
        [p(u, 0, 1), p(b, 0, 2)],
        [p(u, 0, 2), p(b, 0, 1)],
        // DR (D col3 r1/r2 <-> R linha3 c1/c2)
        [p(d, 1, 3), p(rr, 3, 1)],
        [p(d, 2, 3), p(rr, 3, 2)],
        // DF (D linha0 c1/c2 <-> F linha3 c1/c2)
        [p(d, 0, 1), p(f, 3, 1)],
        [p(d, 0, 2), p(f, 3, 2)],
        // DL (D col0 r1/r2 <-> L linha3 c2/c1)
        [p(d, 1, 0), p(l, 3, 2)],
        [p(d, 2, 0), p(l, 3, 1)],
        // DB (D linha3 c1/c2 <-> B linha3 c2/c1)
        [p(d, 3, 1), p(b, 3, 2)],
        [p(d, 3, 2), p(b, 3, 1)],
        // FR (F col3 r1/r2 <-> R col0 r1/r2)
        [p(f, 1, 3), p(rr, 1, 0)],
        [p(f, 2, 3), p(rr, 2, 0)],
        // FL (F col0 r1/r2 <-> L col3 r1/r2)
        [p(f, 1, 0), p(l, 1, 3)],
        [p(f, 2, 0), p(l, 2, 3)],
        // BL (B col3 r1/r2 <-> L col0 r1/r2)
        [p(b, 1, 3), p(l, 1, 0)],
        [p(b, 2, 3), p(l, 2, 0)],
        // BR (B col0 r1/r2 <-> R col3 r1/r2)
        [p(b, 1, 0), p(rr, 1, 3)],
        [p(b, 2, 0), p(rr, 2, 3)],
    ]
}

/// Os 24 encaixes de centro (4 por face).
pub fn center_facelets4() -> [usize; 24] {
    let mut out = [0usize; 24];
    let mut k = 0;
    for f in 0..6 {
        for (r, c) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
            out[k] = p(f, r, c);
            k += 1;
        }
    }
    out
}

pub fn solved4() -> [u8; N_FACELETS4] {
    std::array::from_fn(|i| (i / 16) as u8)
}

pub fn render4(state: &[u8; N_FACELETS4], letters: &[char; 6]) -> String {
    state.iter().map(|&c| letters[c as usize]).collect()
}

/// 96 simbolos -> estado com cores 0..6 no ESQUEMA do canto DBL (como o 2x2).
/// Retorna (estado, letras por face).
pub fn parse4(input: &str) -> Result<([u8; N_FACELETS4], [char; 6]), String> {
    let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != N_FACELETS4 {
        return Err(format!("esperava 96 adesivos, recebi {}", chars.len()));
    }
    let mut colors: Vec<char> = Vec::new();
    for &c in &chars {
        if !colors.contains(&c) {
            colors.push(c);
        }
    }
    if colors.len() != 6 {
        return Err(format!("esperava 6 cores, encontrei {}", colors.len()));
    }
    for &c in &colors {
        let n = chars.iter().filter(|&&x| x == c).count();
        if n != 16 {
            return Err(format!("a cor '{c}' aparece {n} vezes (deveriam ser 16)"));
        }
    }
    let idx_of = |c: char| colors.iter().position(|&x| x == c).unwrap() as u8;
    let raw: Vec<u8> = chars.iter().map(|&c| idx_of(c)).collect();

    // esquema pelo canto DBL + opostos por adjacencia dos cantos
    let cf = corner_facelets4();
    let d_col = raw[cf[6][0]] as usize;
    let b_col = raw[cf[6][1]] as usize;
    let l_col = raw[cf[6][2]] as usize;
    let mut adj = [[false; 6]; 6];
    for t in &cf {
        let (a, b, c) = (raw[t[0]] as usize, raw[t[1]] as usize, raw[t[2]] as usize);
        for (x, y) in [(a, b), (a, c), (b, c)] {
            adj[x][y] = true;
            adj[y][x] = true;
        }
    }
    let opposite = |x: usize| -> Result<usize, String> {
        let mut op = None;
        for y in 0..6 {
            if y != x && !adj[x][y] {
                if op.is_some() {
                    return Err("os cantos nao formam um 4x4 real".into());
                }
                op = Some(y);
            }
        }
        op.ok_or_else(|| "os cantos nao formam um 4x4 real".into())
    };
    let scheme = [opposite(d_col)?, opposite(l_col)?, opposite(b_col)?, d_col, l_col, b_col];
    let mut face_of = [usize::MAX; 6];
    for (face, &col) in scheme.iter().enumerate() {
        face_of[col] = face;
    }
    if face_of.iter().any(|&x| x == usize::MAX) {
        return Err("as cores nao formam um esquema valido".into());
    }
    let mut state = [0u8; N_FACELETS4];
    for i in 0..N_FACELETS4 {
        state[i] = face_of[raw[i] as usize] as u8;
    }
    let letters: [char; 6] = std::array::from_fn(|f| colors[scheme[f]]);

    // validacao de pecas: multiconjunto de asas e cantos igual ao resolvido
    let solved = solved4();
    let wf = wing_facelets4();
    // asas comparadas como PARES NAO ORDENADOS de cores (a quiralidade fina e
    // validada implicitamente pelas etapas da resolucao)
    let pair_of = |s: &[u8; N_FACELETS4], w: &[usize; 2]| {
        let (a, b) = (s[w[0]], s[w[1]]);
        if a <= b { (a, b) } else { (b, a) }
    };
    let mut have: Vec<(u8, u8)> = wf.iter().map(|w| pair_of(&state, w)).collect();
    let mut want: Vec<(u8, u8)> = wf.iter().map(|w| pair_of(&solved, w)).collect();
    have.sort_unstable();
    want.sort_unstable();
    if have != want {
        return Err("as arestas pintadas nao formam um conjunto valido de pecas do 4x4".into());
    }
    let mut tri: Vec<[u8; 3]> = cf.iter().map(|t| [state[t[0]], state[t[1]], state[t[2]]]).collect();
    let mut want_t: Vec<[u8; 3]> =
        cf.iter().map(|t| [solved[t[0]], solved[t[1]], solved[t[2]]]).collect();
    // cantos giram: normaliza cada triplo pela menor rotacao ciclica
    let norm = |t: [u8; 3]| {
        let rots = [t, [t[1], t[2], t[0]], [t[2], t[0], t[1]]];
        *rots.iter().min().unwrap()
    };
    for t in tri.iter_mut() {
        *t = norm(*t);
    }
    for t in want_t.iter_mut() {
        *t = norm(*t);
    }
    tri.sort_unstable();
    want_t.sort_unstable();
    if tri != want_t {
        return Err("os cantos pintados nao formam um conjunto valido de pecas do 4x4".into());
    }
    Ok((state, letters))
}

// ---------------------------------------------------------------------------
// Acao dos movimentos sobre encaixes de centro e de asa + tabelas exatas
// ---------------------------------------------------------------------------

struct Red4 {
    /// cmove[m][encaixe de centro] = destino
    cmove: Vec<[u8; 24]>,
    /// wmove[m][asa] = destino
    wmove: Vec<[u8; 24]>,
    /// wflip[m] bit q: o par de adesivos chega ao destino em ordem trocada?
    /// (medido da geometria das permutacoes, nao de teoria)
    wflip: Vec<u32>,
    /// dist. exata dos 4 centros de uma face ate a face f: [f][rank C(24,4)]
    center_dist: [Vec<u8>; 6],
    /// dist. exata do par de asas ate uma configuracao ALINHADA
    pair_dist: Vec<u8>,
    /// dist. exata CONJUNTA de dois pares (o fim de jogo das "ultimas duas
    /// arestas"): indice = (((a1*24+b1)*2+r1)*576 + (a2*24+b2))*2 + r2
    pair2_dist: Vec<u8>,
}

static RED4: OnceLock<Red4> = OnceLock::new();

fn subset_rank(sorted: &[u8; 4]) -> usize {
    fn cnk(n: usize, k: usize) -> usize {
        if k > n {
            return 0;
        }
        let mut r = 1usize;
        for i in 0..k {
            r = r * (n - i) / (i + 1);
        }
        r
    }
    cnk(sorted[0] as usize, 1)
        + cnk(sorted[1] as usize, 2)
        + cnk(sorted[2] as usize, 3)
        + cnk(sorted[3] as usize, 4)
}

fn red4() -> &'static Red4 {
    RED4.get_or_init(|| {
        let mv = moves4();
        let centers = center_facelets4();
        let wings = wing_facelets4();

        let mut cmove = Vec::with_capacity(N_MOVES4);
        let mut wmove = Vec::with_capacity(N_MOVES4);
        let mut wflip: Vec<u32> = Vec::with_capacity(N_MOVES4);
        for m in 0..N_MOVES4 {
            let perm = &mv.perm[m];
            let mut cm = [0u8; 24];
            for (i, &s) in centers.iter().enumerate() {
                let dst = perm[s] as usize;
                let j = centers.iter().position(|&x| x == dst).expect("centro vira centro");
                cm[i] = j as u8;
            }
            cmove.push(cm);
            let mut wm = [0u8; 24];
            let mut wf = 0u32;
            for (i, w) in wings.iter().enumerate() {
                let (a, b) = (perm[w[0]] as usize, perm[w[1]] as usize);
                let j = wings
                    .iter()
                    .position(|x| (x[0] == a && x[1] == b) || (x[0] == b && x[1] == a))
                    .expect("asa vira asa");
                wm[i] = j as u8;
                if wings[j][0] == b {
                    wf |= 1 << i; // chegou com os adesivos trocados
                }
            }
            wmove.push(wm);
            wflip.push(wf);
        }

        // tabela exata por face: onde estao os 4 centros daquela cor
        let center_dist = std::array::from_fn(|f| {
            let n = 10626; // C(24,4)
            let mut dist = vec![255u8; n];
            let home: [u8; 4] = [4 * f as u8, 4 * f as u8 + 1, 4 * f as u8 + 2, 4 * f as u8 + 3];
            dist[subset_rank(&home)] = 0;
            let mut frontier = vec![home];
            let mut d = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for st in &frontier {
                    for m in 0..N_MOVES4 {
                        let mut v = [
                            cmove[m][st[0] as usize],
                            cmove[m][st[1] as usize],
                            cmove[m][st[2] as usize],
                            cmove[m][st[3] as usize],
                        ];
                        v.sort_unstable();
                        let i = subset_rank(&v);
                        if dist[i] == 255 {
                            dist[i] = d + 1;
                            next.push(v);
                        }
                    }
                }
                frontier = next;
                d += 1;
            }
            dist
        });

        // Tabela exata do par de asas: (posicao A, posicao B, flip relativo).
        // O bit de flip captura o caso "par cruzado" (mesma casa, ordem
        // trocada) — sem ele a heuristica seria zero justamente no osso do
        // fim do pareamento. Objetivo: mesmo encaixe com flip relativo 0.
        let mut pair_dist = vec![255u8; 24 * 24 * 2];
        {
            let idx = |a: usize, b: usize, rel: usize| (a * 24 + b) * 2 + rel;
            let mut frontier = Vec::new();
            for j in 0..12 {
                for (a, b) in [(2 * j, 2 * j + 1), (2 * j + 1, 2 * j)] {
                    pair_dist[idx(a, b, 0)] = 0;
                    frontier.push((a as u8, b as u8, 0u8));
                }
            }
            let mut d = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &(a, b, rel) in &frontier {
                    for m in 0..N_MOVES4 {
                        let (a2, b2) = (wmove[m][a as usize], wmove[m][b as usize]);
                        let fa = (wflip[m] >> a) & 1;
                        let fb = (wflip[m] >> b) & 1;
                        let rel2 = (rel as u32 ^ fa ^ fb) as u8;
                        let i = idx(a2 as usize, b2 as usize, rel2 as usize);
                        if pair_dist[i] == 255 {
                            pair_dist[i] = d + 1;
                            next.push((a2, b2, rel2));
                        }
                    }
                }
                frontier = next;
                d += 1;
            }
        }

        // Tabela conjunta de DOIS pares: 24^2 x 2 x 24^2 x 2 (posicoes podem
        // colidir em indices invalidos, que ficam 255 e nunca sao consultados).
        let mut pair2_dist = vec![255u8; 576 * 2 * 576 * 2];
        {
            let idx = |a1: usize, b1: usize, r1: usize, a2: usize, b2: usize, r2: usize| {
                (((a1 * 24 + b1) * 2 + r1) * 576 + (a2 * 24 + b2)) * 2 + r2
            };
            let mut frontier: Vec<(u8, u8, u8, u8, u8, u8)> = Vec::new();
            for j1 in 0..12usize {
                for (a1, b1) in [(2 * j1, 2 * j1 + 1), (2 * j1 + 1, 2 * j1)] {
                    for j2 in 0..12usize {
                        if j2 == j1 {
                            continue;
                        }
                        for (a2, b2) in [(2 * j2, 2 * j2 + 1), (2 * j2 + 1, 2 * j2)] {
                            let i = idx(a1, b1, 0, a2, b2, 0);
                            if pair2_dist[i] == 255 {
                                pair2_dist[i] = 0;
                                frontier.push((
                                    a1 as u8, b1 as u8, 0, a2 as u8, b2 as u8, 0,
                                ));
                            }
                        }
                    }
                }
            }
            let mut d = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &(a1, b1, r1, a2, b2, r2) in &frontier {
                    for m in 0..N_MOVES4 {
                        let na1 = wmove[m][a1 as usize];
                        let nb1 = wmove[m][b1 as usize];
                        let na2 = wmove[m][a2 as usize];
                        let nb2 = wmove[m][b2 as usize];
                        let f = |q: u8| ((wflip[m] >> q) & 1) as u8;
                        let nr1 = r1 ^ f(a1) ^ f(b1);
                        let nr2 = r2 ^ f(a2) ^ f(b2);
                        let i = idx(
                            na1 as usize,
                            nb1 as usize,
                            nr1 as usize,
                            na2 as usize,
                            nb2 as usize,
                            nr2 as usize,
                        );
                        if pair2_dist[i] == 255 {
                            pair2_dist[i] = d + 1;
                            next.push((na1, nb1, nr1, na2, nb2, nr2));
                        }
                    }
                }
                frontier = next;
                d += 1;
            }
        }

        let r = Red4 { cmove, wmove, wflip, center_dist, pair_dist, pair2_dist };
        // Verificacao empirica da regra compacta das asas (bit de ordem por
        // paridade de encaixe): 300 movimentos aleatorios, compacto == real.
        {
            let mut f = solved4();
            let mut cs = cstate_of(&f);
            let mut seed = 0x1234_5678_9abc_def1u64;
            for _ in 0..300 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let m = ((seed >> 33) % N_MOVES4 as u64) as usize;
                apply_move4(&mut f, m);
                cs = capply(&r, &cs, m);
                assert!(cs == cstate_of(&f), "regra compacta das asas falhou no movimento {m}");
            }
        }
        r
    })
}

// ---------------------------------------------------------------------------
// Predicados sobre a planificacao (usados na certificacao e nos nomes)
// ---------------------------------------------------------------------------

/// type_of[a][b] = tipo de aresta (0..12) do par de cores {a, b}.
fn type_of_colors() -> &'static [[u8; 6]; 6] {
    static T: OnceLock<[[u8; 6]; 6]> = OnceLock::new();
    T.get_or_init(|| {
        let wings = wing_facelets4();
        let solved = solved4();
        let mut t = [[255u8; 6]; 6];
        for k in 0..12 {
            let (a, b) = (solved[wings[2 * k][0]], solved[wings[2 * k][1]]);
            t[a as usize][b as usize] = k as u8;
            t[b as usize][a as usize] = k as u8;
        }
        t
    })
}

/// Aresta k esta pareada (as duas metades de algum encaixe mostram o mesmo par)?
fn edge_paired(state: &[u8; N_FACELETS4], k: usize) -> bool {
    let wings = wing_facelets4();
    let solved = solved4();
    let want = (solved[wings[2 * k][0]], solved[wings[2 * k][1]]);
    for j in 0..12 {
        let s0 = (state[wings[2 * j][0]], state[wings[2 * j][1]]);
        let s1 = (state[wings[2 * j + 1][0]], state[wings[2 * j + 1][1]]);
        if s0 == s1 && (s0 == want || s0 == (want.1, want.0)) {
            return true;
        }
    }
    false
}

fn centers_solved(state: &[u8; N_FACELETS4], faces: &[u8]) -> bool {
    let centers = center_facelets4();
    faces
        .iter()
        .all(|&f| (0..4).all(|i| state[centers[f as usize * 4 + i]] == f))
}

fn paired_count(state: &[u8; N_FACELETS4]) -> usize {
    (0..12).filter(|&k| edge_paired(state, k)).count()
}

// ---------------------------------------------------------------------------
// Estado compacto para as buscas: 24 centros (cor por encaixe) + 24 asas
// (tipo + bit de ordem). Asas nao tem orientacao propria: a ordem mostrada e
// (quiralidade da peca) XOR (paridade do encaixe), entao o bit vira sse a
// paridade do encaixe muda. A regra e verificada contra a planificacao real
// na inicializacao (300 movimentos aleatorios).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
struct CState {
    cent: [u8; 24],
    wt: [u8; 24],
    wo: u32, // bit q = ordem invertida no encaixe q
}

fn cstate_of(state: &[u8; N_FACELETS4]) -> CState {
    let centers = center_facelets4();
    let wings = wing_facelets4();
    let solved = solved4();
    let tmap = type_of_colors();
    let mut cent = [0u8; 24];
    for (i, &s) in centers.iter().enumerate() {
        cent[i] = state[s];
    }
    let mut wt = [0u8; 24];
    let mut wo = 0u32;
    for (q, w) in wings.iter().enumerate() {
        let shown = (state[w[0]], state[w[1]]);
        let t = tmap[shown.0 as usize][shown.1 as usize] as usize;
        wt[q] = t as u8;
        let canon = (solved[wings[2 * t][0]], solved[wings[2 * t][1]]);
        if shown != canon {
            wo |= 1 << q;
        }
    }
    CState { cent, wt, wo }
}

#[inline]
fn capply(r4: &Red4, s: &CState, m: usize) -> CState {
    let mut out = CState { cent: [0; 24], wt: [0; 24], wo: 0 };
    let cm = &r4.cmove[m];
    let wm = &r4.wmove[m];
    for i in 0..24 {
        out.cent[cm[i] as usize] = s.cent[i];
    }
    for q in 0..24 {
        let q2 = wm[q] as usize;
        out.wt[q2] = s.wt[q];
        let flip = (r4.wflip[m] >> q) & 1;
        let bit = ((s.wo >> q) & 1) ^ flip;
        out.wo |= bit << q2;
    }
    out
}

fn c_centers_solved(s: &CState, faces: &[u8]) -> bool {
    faces.iter().all(|&f| (0..4).all(|i| s.cent[f as usize * 4 + i] == f))
}

fn c_center_h(r4: &Red4, s: &CState, faces: &[u8]) -> u8 {
    let mut h = 0u8;
    for &f in faces {
        let mut v = [0u8; 4];
        let mut k = 0;
        for i in 0..24 {
            if s.cent[i] == f {
                v[k] = i as u8;
                k += 1;
            }
        }
        h = h.max(r4.center_dist[f as usize][subset_rank(&v)]);
    }
    h
}

fn c_paired(s: &CState, j: usize) -> Option<u8> {
    // devolve o tipo pareado no ENCAIXE j, se houver
    if s.wt[2 * j] == s.wt[2 * j + 1]
        && ((s.wo >> (2 * j)) & 1) == ((s.wo >> (2 * j + 1)) & 1)
    {
        Some(s.wt[2 * j])
    } else {
        None
    }
}

struct CScan {
    pos: [[u8; 2]; 12],
    paired: [bool; 12],
    count: usize,
}

fn c_scan(s: &CState) -> CScan {
    let mut pos = [[255u8; 2]; 12];
    let mut n = [0usize; 12];
    for q in 0..24 {
        let t = s.wt[q] as usize;
        if n[t] < 2 {
            pos[t][n[t]] = q as u8;
        }
        n[t] += 1;
    }
    let mut paired = [false; 12];
    let mut count = 0;
    for j in 0..12 {
        if let Some(t) = c_paired(s, j) {
            if !paired[t as usize] {
                paired[t as usize] = true;
                count += 1;
            }
        }
    }
    CScan { pos, paired, count }
}

/// Distancia exata conjunta de DOIS pares (fim de jogo do pareamento).
fn c_pair2_h(r4: &Red4, s: &CState, scan: &CScan, k1: usize, k2: usize) -> u8 {
    let rel = |a: usize, b: usize| (((s.wo >> a) ^ (s.wo >> b)) & 1) as usize;
    let idx = |a1: usize, b1: usize, r1: usize, a2: usize, b2: usize, r2: usize| {
        (((a1 * 24 + b1) * 2 + r1) * 576 + (a2 * 24 + b2)) * 2 + r2
    };
    let (p1, q1) = (scan.pos[k1][0] as usize, scan.pos[k1][1] as usize);
    let (p2, q2) = (scan.pos[k2][0] as usize, scan.pos[k2][1] as usize);
    let (r1, r2) = (rel(p1, q1), rel(p2, q2));
    let d = &r4.pair2_dist;
    let mut best = 255u8;
    for (a1, b1) in [(p1, q1), (q1, p1)] {
        for (a2, b2) in [(p2, q2), (q2, p2)] {
            best = best.min(d[idx(a1, b1, r1, a2, b2, r2)]);
        }
    }
    best
}

fn c_pair_h(r4: &Red4, s: &CState, scan: &CScan, k: usize) -> u8 {
    let (a, b) = (scan.pos[k][0] as usize, scan.pos[k][1] as usize);
    let rel = (((s.wo >> a) ^ (s.wo >> b)) & 1) as usize;
    let d = &r4.pair_dist;
    d[(a * 24 + b) * 2 + rel].min(d[(b * 24 + a) * 2 + rel])
}

struct CSearch<'a, G, H>
where
    G: Fn(&CState) -> bool + Sync,
    H: Fn(&CState) -> u8 + Sync,
{
    r4: &'a Red4,
    goal: &'a G,
    h: &'a H,
    path: Vec<usize>,
    nodes: usize,
    stop: &'a std::sync::atomic::AtomicBool,
}

impl<'a, G, H> CSearch<'a, G, H>
where
    G: Fn(&CState) -> bool + Sync,
    H: Fn(&CState) -> u8 + Sync,
{
    fn dfs(&mut self, s: &CState, depth: usize) -> bool {
        self.nodes += 1;
        if self.nodes & 0xFFF == 0 && self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        if self.nodes > 20_000_000 {
            return false;
        }
        let h = (self.h)(s) as usize;
        if h > depth {
            return false;
        }
        if h == 0 && (self.goal)(s) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        for m in 0..N_MOVES4 {
            if let Some(&prev) = self.path.last() {
                if prev % 18 / 3 == m % 18 / 3 && (prev >= 18) == (m >= 18) {
                    continue;
                }
            }
            let s2 = capply(self.r4, s, m);
            self.path.push(m);
            if self.dfs(&s2, depth - 1) {
                return true;
            }
            self.path.pop();
        }
        false
    }

    /// Aprofundamento iterativo com a raiz dividida entre threads: cada uma
    /// pega movimentos iniciais da fila; a primeira que fecha para as demais.
    fn run(r4: &Red4, start: &CState, goal: &G, h: &H, cap: usize) -> Option<Vec<usize>> {
        let h0 = h(start) as usize;
        if h0 == 0 && goal(start) {
            return Some(Vec::new());
        }
        for d in h0.max(1)..=cap {
            if let Some(seq) = Self::run_at(r4, start, goal, h, d) {
                return Some(seq);
            }
        }
        None
    }

    /// Uma unica profundidade (para intercalar alvos diferentes).
    fn run_at(r4: &Red4, start: &CState, goal: &G, h: &H, d: usize) -> Option<Vec<usize>> {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        if h(start) as usize > d {
            return None;
        }
        if d == 0 {
            return if goal(start) { Some(Vec::new()) } else { None };
        }
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 64);
        {
            let found: std::sync::Mutex<Option<Vec<usize>>> = std::sync::Mutex::new(None);
            let stop = AtomicBool::new(false);
            let cursor = AtomicUsize::new(0);
            std::thread::scope(|sc| {
                for _ in 0..workers {
                    let found = &found;
                    let stop = &stop;
                    let cursor = &cursor;
                    sc.spawn(move || loop {
                        let m = cursor.fetch_add(1, Ordering::Relaxed);
                        if m >= N_MOVES4 || stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let s2 = capply(r4, start, m);
                        let mut se =
                            CSearch { r4, goal, h, path: vec![m], nodes: 0, stop };
                        if se.dfs(&s2, d - 1) {
                            let mut g = found.lock().unwrap();
                            if g.is_none() {
                                *g = Some(se.path.clone());
                            }
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                    });
                }
            });
            found.into_inner().unwrap()
        }
    }
}

// ---------------------------------------------------------------------------
// Busca IDA* generica por etapa
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Paridades: candidatos certificados por simulacao
// ---------------------------------------------------------------------------

fn oll_parity_alg() -> &'static Vec<usize> {
    static ALG: OnceLock<Vec<usize>> = OnceLock::new();
    ALG.get_or_init(|| {
        let candidates = [
            "Rw' U2 Lw F2 Lw' F2 Rw2 U2 Rw U2 Rw' U2 F2 Rw2 F2",
            "Rw U2 Rw U2 Rw' U2 Rw U2 Lw' U2 Rw' U2 Rw U2 Rw' U2 Rw'",
        ];
        for cand in candidates {
            let seq = parse_moves4(cand).expect("candidato parseia");
            let mut s = solved4();
            apply_seq4(&mut s, &seq);
            if certify_parity(&s) == Some(ParityKind::Oll) {
                return seq;
            }
        }
        panic!("nenhum candidato de OLL parity certificou");
    })
}

fn pll_parity_alg() -> &'static Vec<usize> {
    static ALG: OnceLock<Vec<usize>> = OnceLock::new();
    ALG.get_or_init(|| {
        let candidates = [
            // classico 2R2 U2 2R2 Uw2 2R2 Uw2 (fatia interna 2R2 = Rw2 R2)
            "Rw2 R2 U2 Rw2 R2 Uw2 Rw2 R2 Uw2",
            "Lw2 L2 U2 Lw2 L2 Uw2 Lw2 L2 Uw2",
            "Rw2 R2 U2 Rw2 R2 Uw2 Rw2 R2 Uw2 U2",
            "Rw2 R2 U2 Rw2 R2 Uw2 U2 Rw2 R2 Uw2 U2",
        ];
        for cand in candidates {
            let seq = parse_moves4(cand).expect("candidato parseia");
            let mut s = solved4();
            apply_seq4(&mut s, &seq);
            if certify_parity(&s) == Some(ParityKind::Pll) {
                return seq;
            }
        }
        panic!("nenhum candidato de PLL parity certificou");
    })
}

#[derive(PartialEq, Debug)]
enum ParityKind {
    Oll,
    Pll,
}

/// O estado (vindo do resolvido + candidato) preserva centros e pares e
/// alterna exatamente uma das paridades do 3x3 reduzido?
fn certify_parity(state: &[u8; N_FACELETS4]) -> Option<ParityKind> {
    if !centers_solved(state, &[0, 1, 2, 3, 4, 5]) {
        return None;
    }
    if !(0..12).all(|k| edge_paired(state, k)) {
        return None;
    }
    let (eo_parity, perm_mismatch) = reduced_parities(state)?;
    match (eo_parity, perm_mismatch) {
        (true, false) => Some(ParityKind::Oll),
        (false, true) => Some(ParityKind::Pll),
        _ => None,
    }
}

/// Do estado reduzido: (soma de orientacao das arestas impar?, paridade de
/// permutacao de cantos != a de arestas?). None se as pecas nao baterem.
fn reduced_parities(state: &[u8; N_FACELETS4]) -> Option<(bool, bool)> {
    let f3 = reduce_to_3x3(state);
    let bytes = f3.as_bytes();
    // extracao manual tolerante (paridades podem estar "impossiveis")
    use crate::facelet::{CORNER_COLOR, CORNER_FACELET, EDGE_COLOR, EDGE_FACELET, FACE_CHARS};
    let col = |i: usize| FACE_CHARS.iter().position(|&c| c == bytes[i]).unwrap();
    let mut cp = [255u8; 8];
    let mut used = [false; 8];
    for i in 0..8 {
        let mut ori = 4;
        for o in 0..3 {
            let c = col(CORNER_FACELET[i][o]);
            if c == 0 || c == 3 {
                ori = o;
                break;
            }
        }
        if ori == 4 {
            return None;
        }
        let c1 = col(CORNER_FACELET[i][(ori + 1) % 3]);
        let c2 = col(CORNER_FACELET[i][(ori + 2) % 3]);
        let j = (0..8).find(|&j| c1 == CORNER_COLOR[j][1] && c2 == CORNER_COLOR[j][2])?;
        if used[j] {
            return None;
        }
        used[j] = true;
        cp[i] = j as u8;
    }
    let mut ep = [255u8; 12];
    let mut eo_sum = 0usize;
    let mut used_e = [false; 12];
    for i in 0..12 {
        let a = col(EDGE_FACELET[i][0]);
        let b = col(EDGE_FACELET[i][1]);
        let mut hit = None;
        for j in 0..12 {
            if a == EDGE_COLOR[j][0] && b == EDGE_COLOR[j][1] {
                hit = Some((j, 0));
                break;
            }
            if a == EDGE_COLOR[j][1] && b == EDGE_COLOR[j][0] {
                hit = Some((j, 1));
                break;
            }
        }
        let (j, o) = hit?;
        if used_e[j] {
            return None;
        }
        used_e[j] = true;
        ep[i] = j as u8;
        eo_sum += o;
    }
    let parity = |p: &[u8]| {
        let mut s = 0;
        for i in 0..p.len() {
            for j in (i + 1)..p.len() {
                if p[j] < p[i] {
                    s += 1;
                }
            }
        }
        s % 2
    };
    Some((eo_sum % 2 == 1, parity(&cp) != parity(&ep)))
}

/// Estado 4x4 reduzido -> planificacao 3x3 (letras U..B).
fn reduce_to_3x3(state: &[u8; N_FACELETS4]) -> String {
    use crate::facelet::{CORNER_FACELET, EDGE_FACELET, FACE_CHARS};
    let cf4 = corner_facelets4();
    let wf = wing_facelets4();
    let mut out = [b'?'; 54];
    for f in 0..6 {
        out[f * 9 + 4] = FACE_CHARS[f];
    }
    for i in 0..8 {
        for k in 0..3 {
            out[CORNER_FACELET[i][k]] = FACE_CHARS[state[cf4[i][k]] as usize];
        }
    }
    for i in 0..12 {
        out[EDGE_FACELET[i][0]] = FACE_CHARS[state[wf[2 * i][0]] as usize];
        out[EDGE_FACELET[i][1]] = FACE_CHARS[state[wf[2 * i][1]] as usize];
    }
    String::from_utf8(out.to_vec()).unwrap()
}

// ---------------------------------------------------------------------------
// Pipeline de reducao
// ---------------------------------------------------------------------------

pub struct Stage4 {
    pub name: String,
    pub info: String,
    pub tokens: Vec<String>,
}

pub struct Solve4 {
    pub stages: Vec<Stage4>,
    pub states: Vec<String>,
    pub length: usize,
}

const EDGE_NAMES4: [&str; 12] = [
    "cima-direita", "cima-frente", "cima-esquerda", "cima-trás",
    "baixo-direita", "baixo-frente", "baixo-esquerda", "baixo-trás",
    "frente-direita", "frente-esquerda", "trás-esquerda", "trás-direita",
];

pub fn solve4(input: &str, t: &Tables) -> Result<Solve4, String> {
    let (mut state, letters) = parse4(input)?;
    let _ = red4();
    let mut stages: Vec<Stage4> = Vec::new();
    let mut states = vec![render4(&state, &letters)];

    let push_stage =
        |state: &mut [u8; N_FACELETS4],
         states: &mut Vec<String>,
         stages: &mut Vec<Stage4>,
         name: String,
         info: String,
         seq: &[usize]| {
            if seq.is_empty() {
                return;
            }
            let mut tokens = Vec::new();
            for &m in seq {
                apply_move4(state, m);
                states.push(render4(state, &letters));
                tokens.push(move_name4(m));
            }
            stages.push(Stage4 { name, info, tokens });
        };

    // ---- centros, cor a cor (a ultima vem de graca) ---------------------
    let r4 = red4();
    let center_order: [u8; 5] = [0, 3, 2, 5, 4]; // U D F B L (R forcado)
    let face_names = ["de cima", "da direita", "da frente", "de baixo", "da esquerda", "de trás"];
    let mut done_faces: Vec<u8> = Vec::new();
    for &f in &center_order {
        let cs = cstate_of(&state);
        if c_centers_solved(&cs, &[f]) {
            done_faces.push(f);
            continue;
        }
        let mut need = done_faces.clone();
        need.push(f);
        let goal = |s: &CState| c_centers_solved(s, &need);
        let h = |s: &CState| c_center_h(r4, s, &need);
        let seq = CSearch::run(r4, &cs, &goal, &h, 13)
            .ok_or_else(|| format!("nao achei os centros da face {}", face_names[f as usize]))?;
        push_stage(
            &mut state,
            &mut states,
            &mut stages,
            format!("Centros {}", face_names[f as usize]),
            format!("Junte os 4 centros da face {} no lugar.", face_names[f as usize]),
            &seq,
        );
        done_faces.push(f);
    }
    if !centers_solved(&state, &[0, 1, 2, 3, 4, 5]) {
        return Err("erro interno: centros nao fecharam".into());
    }

    // ---- pareamento das 12 arestas --------------------------------------
    // Duas marchas: primeiro uma busca rasa por QUALQUER avanco no numero de
    // pares (o jeito "freeslice": uma fatiada costuma parear varios de uma
    // vez); quando ela nao rende, busca dirigida a um par especifico com a
    // heuristica exata (que tambem resolve o caso do ultimo par cruzado).
    let all_faces: [u8; 6] = [0, 1, 2, 3, 4, 5];
    let mut guard = 0;
    loop {
        let count = paired_count(&state);
        if count == 12 {
            break;
        }
        guard += 1;
        if guard > 40 {
            return Err("pareamento nao convergiu (estado impossivel?)".into());
        }

        // marcha 1: qualquer +1, com teto curto (o minimo exato + 2): pega os
        // ganhos multiplos baratos sem cavar fundo com heuristica fraca
        let cs = cstate_of(&state);

        let goal_any = |s: &CState| {
            c_centers_solved(s, &all_faces) && c_scan(s).count > count
        };
        let h_any = |s: &CState| {
            let hc = c_center_h(r4, s, &all_faces);
            let scan = c_scan(s);
            if scan.count > count {
                return hc;
            }
            // para a contagem subir, ALGUM tipo hoje solto precisa parear
            let mut hp = 255u8;
            for k in 0..12 {
                if !scan.paired[k] {
                    hp = hp.min(c_pair_h(r4, s, &scan, k));
                }
            }
            hc.max(if hp == 255 { 0 } else { hp })
        };
        if let Some(seq) = CSearch::run(r4, &cs, &goal_any, &h_any, 7) {
            let before: Vec<usize> = (0..12).filter(|&k| edge_paired(&state, k)).collect();
            push_stage(
                &mut state,
                &mut states,
                &mut stages,
                String::new(), // nome depois
                String::new(),
                &seq,
            );
            // renomeia com o que foi pareado
            let after: Vec<usize> = (0..12).filter(|&k| edge_paired(&state, k)).collect();
            let novos: Vec<&str> = after
                .iter()
                .filter(|k| !before.contains(k))
                .map(|&k| EDGE_NAMES4[k])
                .collect();
            if let Some(st) = stages.last_mut() {
                st.name = format!("Parear aresta {}", novos.join(" e "));
                st.info = "Junte as duas metades de cada aresta (fatia, encaixa, desfaz a fatia).".into();
            }
            continue;
        }

        // marcha 2: alvos especificos com profundidades INTERCALADAS (nao
        // afundar num alvo dificil antes de tentar o facil mais fundo).
        // Fim de jogo (<= 2 soltos): objetivo unico "todos pareados", com a
        // heuristica maxima dos que faltam.
        let root_scan = c_scan(&cs);
        let paired_now: Vec<usize> = (0..12).filter(|&t| root_scan.paired[t]).collect();
        let mut targets: Vec<usize> = (0..12).filter(|&t| !root_scan.paired[t]).collect();
        targets.sort_by_key(|&k| c_pair_h(r4, &cs, &root_scan, k));
        let endgame = targets.len() <= 2;

        let mut found: Option<(String, Vec<usize>)> = None;
        'depths: for d in 1..=16usize {
            if endgame {
                let alvo = targets.clone();
                let goal = |s: &CState| {
                    if !c_centers_solved(s, &all_faces) {
                        return false;
                    }
                    let sc = c_scan(s);
                    sc.count == 12
                };
                let h = |s: &CState| {
                    let sc = c_scan(s);
                    let mut hh = c_center_h(r4, s, &all_faces);
                    let soltos: Vec<usize> = (0..12).filter(|&k| !sc.paired[k]).collect();
                    match soltos.len() {
                        0 => {}
                        1 => hh = hh.max(c_pair_h(r4, s, &sc, soltos[0])),
                        n if n <= 4 => {
                            // maximo sobre TODAS as combinacoes de dois pares:
                            // pune ramos que quebram os ja pareados
                            for i in 0..n {
                                for j in (i + 1)..n {
                                    hh = hh.max(c_pair2_h(r4, s, &sc, soltos[i], soltos[j]));
                                }
                            }
                        }
                        _ => {
                            hh = hh.max(c_pair2_h(r4, s, &sc, soltos[0], soltos[1]));
                            for &k in &soltos[2..] {
                                hh = hh.max(c_pair_h(r4, s, &sc, k));
                            }
                        }
                    }
                    hh
                };
                if let Some(seq) = CSearch::run_at(r4, &cs, &goal, &h, d) {
                    let nomes: Vec<&str> =
                        alvo.iter().map(|&k| EDGE_NAMES4[k]).collect();
                    found = Some((
                        format!("Parear aresta {}", nomes.join(" e ")),
                        seq,
                    ));
                    break 'depths;
                }
            } else {
                for &k in &targets {
                    let goal = |s: &CState| {
                        if !c_centers_solved(s, &all_faces) {
                            return false;
                        }
                        let sc = c_scan(s);
                        sc.paired[k] && paired_now.iter().all(|&j| sc.paired[j])
                    };
                    let h = |s: &CState| {
                        let sc = c_scan(s);
                        c_center_h(r4, s, &all_faces).max(c_pair_h(r4, s, &sc, k))
                    };
                    if let Some(seq) = CSearch::run_at(r4, &cs, &goal, &h, d) {
                        found = Some((
                            format!("Parear aresta {}", EDGE_NAMES4[k]),
                            seq,
                        ));
                        break 'depths;
                    }
                }
            }
        }
        match found {
            Some((nome, seq)) => {
                push_stage(
                    &mut state,
                    &mut states,
                    &mut stages,
                    nome,
                    "Junte as duas metades da aresta.".into(),
                    &seq,
                );
            }
            None => {
                let sc = c_scan(&cs);
                let hs: Vec<String> = (0..12)
                    .filter(|&k| !sc.paired[k])
                    .map(|k| format!("{}:h{}", k, c_pair_h(r4, &cs, &sc, k)))
                    .collect();
                return Err(format!(
                    "nao consegui parear (soltos: {}; estado: {})",
                    hs.join(" "),
                    render4(&state, &['U', 'R', 'F', 'D', 'L', 'B'])
                ));
            }
        }
    }

    // ---- paridades + resolver como 3x3 ----------------------------------
    let (eo_bad, perm_bad) =
        reduced_parities(&state).ok_or("erro interno: reducao com pecas invalidas")?;
    if eo_bad {
        let alg = oll_parity_alg().clone();
        push_stage(
            &mut state,
            &mut states,
            &mut stages,
            "Paridade de orientação (OLL parity)".into(),
            "O 4x4 permite uma aresta 'virada' que não existe no 3x3; este algoritmo conserta.".into(),
            &alg,
        );
    }
    let (_, perm_bad2) =
        reduced_parities(&state).ok_or("erro interno: reducao apos OLL parity")?;
    if perm_bad || perm_bad2 {
        if perm_bad2 {
            let alg = pll_parity_alg().clone();
            push_stage(
                &mut state,
                &mut states,
                &mut stages,
                "Paridade de permutação (PLL parity)".into(),
                "Duas peças trocadas que não existem no 3x3; este algoritmo conserta.".into(),
                &alg,
            );
        }
    }

    let f3 = reduce_to_3x3(&state);
    let cube3 = crate::facelet::to_cubie(&f3)
        .map_err(|e| format!("reducao nao virou um 3x3 valido: {e}"))?;
    let sol3 = search::solve(
        &cube3,
        t,
        SolveParams { max_len: 21, target_len: 21, timeout_ms: 2000, min_ms: 0, threads: search::default_threads() },
    )?;
    let seq: Vec<usize> = sol3.moves.iter().map(|&m| m as usize).collect();
    push_stage(
        &mut state,
        &mut states,
        &mut stages,
        "Resolver como 3x3".into(),
        "Com centros prontos e arestas pareadas, o 4x4 vira um 3x3: só giros externos.".into(),
        &seq,
    );

    if state != solved4() {
        return Err("erro interno: o 4x4 nao fechou".into());
    }

    // ---- limpeza final ---------------------------------------------------
    // As etapas sao resolvidas uma a uma, entao sobram redundancias na emenda
    // (`U U` em vez de `U2`, `R L R'` em vez de `L`). So aceitamos o resultado
    // se ele continuar resolvendo o cubo.
    {
        let mut planos: Vec<(usize, usize)> = Vec::new();
        for (si, st) in stages.iter().enumerate() {
            for tk in &st.tokens {
                if let Ok(ms) = parse_moves4(tk) {
                    for m in ms {
                        planos.push((m, si));
                    }
                }
            }
        }
        let camada = |m: usize| if m < 18 { m / 3 } else { 6 + (m - 18) / 3 };
        let eixo = |m: usize| (if m < 18 { m / 3 } else { (m - 18) / 3 }) % 3;
        let monta = |c: usize, p: usize| if c < 6 { c * 3 + p } else { 18 + (c - 6) * 3 + p };
        let so_movs: Vec<usize> = planos.iter().map(|&(m, _)| m).collect();
        let limpo = crate::simplify::simplify(&so_movs, camada, eixo, monta);
        if limpo.len() < so_movs.len() {
            // reconstroi etapas e estados com a lista enxuta
            let etapa_de = |i: usize| planos.get(i).map(|&(_, s)| s).unwrap_or(0);
            let nomes: Vec<(String, String)> =
                stages.iter().map(|s| (s.name.clone(), s.info.clone())).collect();
            let (mut novo, _) = parse4(input)?;
            let mut novos: Vec<Stage4> = Vec::new();
            let mut novos_estados = vec![render4(&novo, &letters)];
            for (i, &m) in limpo.iter().enumerate() {
                apply_move4(&mut novo, m);
                novos_estados.push(render4(&novo, &letters));
                let si = etapa_de(i.min(planos.len().saturating_sub(1)));
                let nome = nomes.get(si).map(|x| x.0.clone()).unwrap_or_default();
                match novos.last_mut() {
                    Some(u) if u.name == nome => u.tokens.push(move_name4(m)),
                    _ => novos.push(Stage4 {
                        name: nome,
                        info: nomes.get(si).map(|x| x.1.clone()).unwrap_or_default(),
                        tokens: vec![move_name4(m)],
                    }),
                }
            }
            if novo == solved4() {
                stages = novos;
                states = novos_estados;
            }
        }
    }

    let length = stages.iter().map(|s| s.tokens.len()).sum();
    Ok(Solve4 { stages, states, length })
}

// ---------------------------------------------------------------------------
// Preenchimento parcial (modo guiado do 4x4).
//
// Restricoes reais do 4x4 (com centros indistinguiveis, todas as permutacoes
// sao alcancaveis): conjunto de pecas de canto + soma de orientacoes (mod 3);
// asas = emparelhamento perfeito peca <-> encaixe respeitando a ORDEM mostrada
// (deduzida da geometria: a peca w no encaixe q mostra uma ordem determinada);
// centros = no maximo 4 por cor.
// ---------------------------------------------------------------------------

/// ord bit: a peca w (asa) no encaixe q mostra as cores em ordem trocada?
fn wing_ord_table() -> &'static [[u8; 24]; 24] {
    static T: OnceLock<[[u8; 24]; 24]> = OnceLock::new();
    T.get_or_init(|| {
        let r4 = red4();
        let mut ord = [[255u8; 24]; 24];
        for w in 0..24 {
            // BFS da peca a partir de casa (ordem canonica)
            let mut vis = [false; 24];
            let mut queue = vec![(w, 0u8)];
            ord[w][w] = 0;
            vis[w] = true;
            while let Some((q, ob)) = queue.pop() {
                for m in 0..N_MOVES4 {
                    let q2 = r4.wmove[m][q] as usize;
                    let ob2 = ob ^ (((r4.wflip[m] >> q) & 1) as u8);
                    if !vis[q2] {
                        vis[q2] = true;
                        ord[w][q2] = ob2;
                        queue.push((q2, ob2));
                    } else {
                        assert_eq!(
                            ord[w][q2], ob2,
                            "ordem da asa {w} no encaixe {q2} ambigua"
                        );
                    }
                }
            }
            assert!(vis.iter().all(|&v| v), "asa {w} nao alcanca todos os encaixes");
        }
        ord
    })
}

fn feasible4(f: &[Option<u8>; N_FACELETS4]) -> bool {
    // contagens
    for c in 0..6u8 {
        if f.iter().filter(|&&x| x == Some(c)).count() > 16 {
            return false;
        }
    }
    let centers = center_facelets4();
    for c in 0..6u8 {
        if centers.iter().filter(|&&s| f[s] == Some(c)).count() > 4 {
            return false;
        }
    }

    // cantos: mesmo maquinario do 2x2/3x3 (paridade nao restringe)
    use crate::facelet::CORNER_COLOR;
    let cf = corner_facelets4();
    let mut cand: Vec<Vec<(u8, i8)>> = Vec::with_capacity(8);
    let mut has_free = false;
    for slot in 0..8 {
        let painted = cf[slot].iter().any(|&p| f[p].is_some());
        if !painted {
            has_free = true;
        }
        let mut v = Vec::new();
        for piece in 0..8 {
            for o in 0..3usize {
                let ok = (0..3).all(|k| match f[cf[slot][(k + o) % 3]] {
                    Some(col) => col as usize == CORNER_COLOR[piece][k],
                    None => true,
                });
                if ok {
                    if painted {
                        v.push((piece as u8, o as i8));
                    } else {
                        v.push((piece as u8, -1));
                        break;
                    }
                }
            }
        }
        if v.is_empty() {
            return false;
        }
        cand.push(v);
    }
    let sc = crate::partial::achievable(&cand, has_free, 3);
    if !sc[0] && !sc[1] {
        return false;
    }

    // asas: emparelhamento perfeito peca <-> encaixe respeitando a ordem
    let wf = wing_facelets4();
    let solved = solved4();
    let ord = wing_ord_table();
    let fits = |w: usize, q: usize| -> bool {
        let t = w / 2;
        let canon = (solved[wf[2 * t][0]], solved[wf[2 * t][1]]);
        let shown = if ord[w][q] == 0 { canon } else { (canon.1, canon.0) };
        let ok0 = match f[wf[q][0]] {
            Some(col) => col == shown.0,
            None => true,
        };
        let ok1 = match f[wf[q][1]] {
            Some(col) => col == shown.1,
            None => true,
        };
        ok0 && ok1
    };
    // Kuhn: casa cada peca num encaixe
    let mut match_slot = [usize::MAX; 24]; // encaixe -> peca
    fn try_kuhn(
        w: usize,
        fits: &dyn Fn(usize, usize) -> bool,
        seen: &mut [bool; 24],
        match_slot: &mut [usize; 24],
    ) -> bool {
        for q in 0..24 {
            if fits(w, q) && !seen[q] {
                seen[q] = true;
                if match_slot[q] == usize::MAX
                    || try_kuhn(match_slot[q], fits, seen, match_slot)
                {
                    match_slot[q] = w;
                    return true;
                }
            }
        }
        false
    }
    for w in 0..24 {
        let mut seen = [false; 24];
        if !try_kuhn(w, &fits, &mut seen, &mut match_slot) {
            return false;
        }
    }
    true
}

/// Cores possiveis (indices 0..6 do esquema padrao) na posicao `pos` de um
/// preenchimento parcial ('.' = vazio).
pub fn allowed_colors4(input: &str, pos: usize) -> Result<Vec<usize>, String> {
    let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != N_FACELETS4 {
        return Err(format!("esperava 96 simbolos, recebi {}", chars.len()));
    }
    if pos >= N_FACELETS4 {
        return Err("posicao invalida".into());
    }
    let mut f = [None; N_FACELETS4];
    for (i, &c) in chars.iter().enumerate() {
        if c == '.' {
            continue;
        }
        match "URFDLB".chars().position(|x| x == c) {
            Some(k) => f[i] = Some(k as u8),
            None => return Err(format!("simbolo desconhecido '{c}'")),
        }
    }
    let mut out = Vec::new();
    for c in 0..6u8 {
        let mut g = f;
        g[pos] = Some(c);
        if feasible4(&g) {
            out.push(c as usize);
        }
    }
    Ok(out)
}

/// Embaralhamento por movimentos aleatorios.
pub fn scramble4(mut rand: impl FnMut(u64) -> u64) -> (String, String) {
    let mut state = solved4();
    let mut tokens = Vec::new();
    let mut last_face = 99usize;
    let mut n = 0;
    while n < 40 {
        let m = rand(N_MOVES4 as u64) as usize;
        let face = m % 18 / 3;
        if face == last_face {
            continue;
        }
        last_face = face;
        apply_move4(&mut state, m);
        tokens.push(move_name4(m));
        n += 1;
    }
    (
        render4(&state, &['U', 'R', 'F', 'D', 'L', 'B']),
        tokens.join(" "),
    )
}

/// Aplica notacao sobre 96 adesivos pintados.
pub fn apply4(input: &str, moves_str: &str) -> Result<String, String> {
    let (mut state, letters) = parse4(input)?;
    let seq = parse_moves4(moves_str)?;
    apply_seq4(&mut state, &seq);
    Ok(render4(&state, &letters))
}
