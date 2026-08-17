//! Cubo 2x2 (Pocket Cube): solucao OTIMA sempre.
//!
//! Um 2x2 e so os 8 cantos do 3x3 — a algebra de cantos existente serve
//! inteira. Sem centros, o referencial vem do canto no encaixe DBL: ele define
//! as cores de D, B e L (e os opostos definem U, R e F), e fica parado porque
//! so usamos os movimentos U, R e F. Sobram 7 pecas x permutacao (5040) x
//! orientacao (3^6 = 729) = 3.674.160 estados: a tabela de Deus completa cabe
//! em 3,7 MB e e gerada por BFS no boot (< 1 s). Toda solucao e otima
//! (maximo 11 movimentos, o numero de Deus do 2x2).

use std::sync::OnceLock;

use crate::coord::{perm_from_index, perm_index};
use crate::cube::{CubieCube, SOLVED};
use crate::tables::Tables;

/// Adesivos de cada canto na planificacao de 24 (faces U R F D L B, 2x2 cada,
/// leitura em linhas). Mesma ordem de cores do CORNER_COLOR do 3x3.
pub const CORNER_FACELET2: [[usize; 3]; 8] = [
    [3, 4, 9],   // URF
    [2, 8, 13],  // UFL
    [0, 12, 21], // ULB
    [1, 20, 5],  // UBR
    [17, 11, 6], // DFR
    [16, 15, 10],// DLF
    [18, 23, 14],// DBL
    [19, 7, 22], // DRB
];

/// Cores de cada canto (indices de face 0..6), identico ao 3x3.
const CORNER_COLOR2: [[usize; 3]; 8] = [
    [0, 1, 2],
    [0, 2, 4],
    [0, 4, 5],
    [0, 5, 1],
    [3, 2, 1],
    [3, 4, 2],
    [3, 5, 4],
    [3, 1, 5],
];

/// Movimentos permitidos (mantem o canto DBL parado): U, U2, U', R..., F...
const MOVES2: [u8; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];
pub const N_STATES2: usize = 5040 * 729;

static GOD2: OnceLock<Vec<u8>> = OnceLock::new();

fn god_table(t: &Tables) -> &'static Vec<u8> {
    GOD2.get_or_init(|| {
        let mut dist = vec![255u8; N_STATES2];
        dist[index2(&SOLVED)] = 0;
        let mut frontier = vec![SOLVED];
        let mut d = 0u8;
        while !frontier.is_empty() {
            let mut next = Vec::with_capacity(frontier.len() * 3);
            for c in &frontier {
                for &m in &MOVES2 {
                    let c2 = corner_mult(c, &t.mc[m as usize]);
                    let i = index2(&c2);
                    if dist[i] == 255 {
                        dist[i] = d + 1;
                        next.push(c2);
                    }
                }
            }
            frontier = next;
            d += 1;
        }
        assert!(d <= 12, "BFS do 2x2 passou do numero de Deus");
        assert!(dist.iter().all(|&v| v != 255), "estado 2x2 inalcancavel");
        dist
    })
}

/// Multiplicacao so de cantos (arestas ignoradas).
fn corner_mult(a: &CubieCube, b: &CubieCube) -> CubieCube {
    let mut r = SOLVED;
    for i in 0..8 {
        let k = b.cp[i] as usize;
        r.cp[i] = a.cp[k];
        r.co[i] = (a.co[k] + b.co[i]) % 3;
    }
    r
}

/// Indice do estado: permutacao dos 7 cantos moveis x orientacao de 6.
fn index2(c: &CubieCube) -> usize {
    // encaixes moveis: 0..6 e 7 (DBL = 6 fica parado)
    let mut p = [0u8; 7];
    let mut k = 0;
    for slot in [0usize, 1, 2, 3, 4, 5, 7] {
        let piece = c.cp[slot];
        p[k] = if piece == 7 { 6 } else { piece };
        k += 1;
    }
    let mut tw = 0usize;
    for slot in [0usize, 1, 2, 3, 4, 5] {
        tw = tw * 3 + c.co[slot] as usize;
    }
    perm_index(&p) as usize * 729 + tw
}

/// Interpreta 24 adesivos. O canto em DBL define o esquema de cores.
/// Retorna o estado (cantos) e o esquema (cor de cada face).
pub fn parse2(input: &str) -> Result<(CubieCube, [usize; 6]), String> {
    let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != 24 {
        return Err(format!("esperava 24 adesivos, recebi {}", chars.len()));
    }
    // identifica as 6 cores
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
        if n != 4 {
            return Err(format!("a cor '{c}' aparece {n} vezes (deveriam ser 4)"));
        }
    }
    let idx_of = |c: char| colors.iter().position(|&x| x == c).unwrap();
    let f: Vec<usize> = chars.iter().map(|&c| idx_of(c)).collect();

    // ancora: canto DBL define D, B e L
    let d_col = f[CORNER_FACELET2[6][0]];
    let b_col = f[CORNER_FACELET2[6][1]];
    let l_col = f[CORNER_FACELET2[6][2]];
    if d_col == b_col || d_col == l_col || b_col == l_col {
        return Err("o canto de baixo-tras-esquerda tem cores repetidas".into());
    }
    // opostos: a cor que nunca divide um canto com X e a oposta de X
    let mut adj = [[false; 6]; 6];
    for cf in &CORNER_FACELET2 {
        let (a, b, c) = (f[cf[0]], f[cf[1]], f[cf[2]]);
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
                    return Err("as cores nao formam um 2x2 real (opostos ambiguos)".into());
                }
                op = Some(y);
            }
        }
        op.ok_or_else(|| "as cores nao formam um 2x2 real (sem cor oposta)".into())
    };
    let u_col = opposite(d_col)?;
    let r_col = opposite(l_col)?;
    let f_col = opposite(b_col)?;

    // scheme[face] = cor
    let scheme = [u_col, r_col, f_col, d_col, l_col, b_col];
    // color -> face
    let mut face_of = [usize::MAX; 6];
    for (face, &col) in scheme.iter().enumerate() {
        face_of[col] = face;
    }
    if face_of.iter().any(|&x| x == usize::MAX) {
        return Err("as cores nao formam um esquema valido".into());
    }

    // cantos como no 3x3
    let mut c = SOLVED;
    let mut used = [false; 8];
    for i in 0..8 {
        let mut ori = 3;
        for o in 0..3 {
            let col = face_of[f[CORNER_FACELET2[i][o]]];
            if col == 0 || col == 3 {
                ori = o;
                break;
            }
        }
        if ori == 3 {
            return Err(format!("o canto {i} nao tem a cor de cima nem a de baixo"));
        }
        let c1 = face_of[f[CORNER_FACELET2[i][(ori + 1) % 3]]];
        let c2 = face_of[f[CORNER_FACELET2[i][(ori + 2) % 3]]];
        let mut found = None;
        for j in 0..8 {
            if c1 == CORNER_COLOR2[j][1] && c2 == CORNER_COLOR2[j][2] {
                found = Some(j);
                break;
            }
        }
        match found {
            Some(j) => {
                if used[j] {
                    return Err(format!("a peca do canto {j} esta repetida"));
                }
                used[j] = true;
                c.cp[i] = j as u8;
                c.co[i] = ori as u8;
            }
            None => return Err(format!("o canto {i} nao existe num 2x2 real")),
        }
    }
    let twist: u32 = c.co.iter().map(|&x| x as u32).sum();
    if twist % 3 != 0 {
        return Err("um canto esta torcido (orientacao impossivel)".into());
    }
    // com a ancora em DBL o proprio canto ja sai resolvido
    debug_assert_eq!(c.cp[6], 6);
    debug_assert_eq!(c.co[6], 0);
    Ok((c, scheme))
}

/// Estado (cantos) -> 24 adesivos, usando as letras de cor dadas por face.
pub fn render2(c: &CubieCube, letters: &[char; 6]) -> String {
    let mut out = ['?'; 24];
    for i in 0..8 {
        let j = c.cp[i] as usize;
        let ori = c.co[i] as usize;
        for k in 0..3 {
            out[CORNER_FACELET2[i][(k + ori) % 3]] = letters[CORNER_COLOR2[j][k]];
        }
    }
    out.iter().collect()
}

pub struct Solve2 {
    pub moves: Vec<u8>,
    pub states: Vec<String>,
    pub length: usize,
}

/// Solucao otima, descendo pela tabela de Deus.
pub fn solve2(input: &str, t: &Tables) -> Result<Solve2, String> {
    let (mut c, scheme) = parse2(input)?;
    // letras no referencial da ancora: a cor da face f e a letra pintada
    let chars: Vec<char> = input.trim().chars().filter(|x| !x.is_whitespace()).collect();
    let mut colors: Vec<char> = Vec::new();
    for &x in &chars {
        if !colors.contains(&x) {
            colors.push(x);
        }
    }
    let letters: [char; 6] = std::array::from_fn(|f| colors[scheme[f]]);

    let god = god_table(t);
    let mut moves = Vec::new();
    let mut states = vec![render2(&c, &letters)];
    let mut cur = god[index2(&c)];
    while cur > 0 {
        let mut advanced = false;
        for &m in &MOVES2 {
            let c2 = corner_mult(&c, &t.mc[m as usize]);
            if god[index2(&c2)] == cur - 1 {
                moves.push(m);
                c = c2;
                states.push(render2(&c, &letters));
                cur -= 1;
                advanced = true;
                break;
            }
        }
        if !advanced {
            return Err("erro interno: tabela do 2x2 sem descida".into());
        }
    }
    debug_assert!(c.cp.iter().enumerate().all(|(i, &p)| p as usize == i));
    let length = moves.len();
    Ok(Solve2 { moves, states, length })
}

/// Embaralhamento: estado uniforme entre os 3.674.160.
pub fn scramble2(t: &Tables, mut rand: impl FnMut(u64) -> u64) -> String {
    let perm = rand(5040) as u32;
    let tw = rand(729) as usize;
    let mut c = SOLVED;
    let mut p7 = [0u8; 7];
    perm_from_index(perm, 7, &mut p7);
    let slots = [0usize, 1, 2, 3, 4, 5, 7];
    for (k, &slot) in slots.iter().enumerate() {
        c.cp[slot] = if p7[k] == 6 { 7 } else { p7[k] };
    }
    let mut v = tw;
    let mut sum = 0u32;
    for slot in [5usize, 4, 3, 2, 1, 0] {
        c.co[slot] = (v % 3) as u8;
        sum += c.co[slot] as u32;
        v /= 3;
    }
    c.co[7] = ((3 - sum % 3) % 3) as u8;
    let _ = god_table(t); // garante a tabela pronta
    render2(&c, &['U', 'R', 'F', 'D', 'L', 'B'])
}

/// Aplica uma sequencia (U R F D L B e variantes) sobre 24 adesivos.
pub fn apply2(input: &str, moves: &[u8], t: &Tables) -> Result<String, String> {
    let (mut c, scheme) = parse2(input)?;
    let chars: Vec<char> = input.trim().chars().filter(|x| !x.is_whitespace()).collect();
    let mut colors: Vec<char> = Vec::new();
    for &x in &chars {
        if !colors.contains(&x) {
            colors.push(x);
        }
    }
    let letters: [char; 6] = std::array::from_fn(|f| colors[scheme[f]]);
    for &m in moves {
        c = corner_mult(&c, &t.mc[m as usize]);
    }
    Ok(render2(&c, &letters))
}
