//! Analise de planificacoes PARCIAIS (com adesivos ainda nao pintados).
//!
//! A pergunta central: dado um preenchimento parcial, quais cores podem entrar
//! numa posicao sem tornar o cubo impossivel? Uma cor e permitida quando existe
//! ALGUMA forma de completar o resto formando um estado fisicamente valido:
//!
//!   - cada peca (8 cantos, 12 arestas) usada exatamente uma vez, encaixando
//!     nos adesivos ja pintados (emparelhamento perfeito peca <-> encaixe);
//!   - soma das orientacoes dos cantos = 0 (mod 3) e das arestas = 0 (mod 2)
//!     — um encaixe totalmente vazio absorve qualquer soma, senao as
//!     orientacoes ficam todas forcadas pelos adesivos;
//!   - paridade da permutacao dos cantos igual a das arestas.
//!
//! Os espacos sao minusculos (8 e 12 encaixes), entao uma busca com poda que
//! coleta as paridades alcancaveis resolve em microssegundos: os casos com
//! muita liberdade acham um completamento imediatamente, e os casos apertados
//! tem pouquissimos candidatos para tentar.

use crate::facelet::{CORNER_COLOR, CORNER_FACELET, EDGE_COLOR, EDGE_FACELET, FACE_NAMES};

pub const UNKNOWN: char = '.';

/// Interpreta uma planificacao parcial: 54 simbolos, centros obrigatorios e
/// distintos (definem qual cor e qual face), '.' = ainda nao pintado.
pub fn parse_partial(input: &str) -> Result<[Option<usize>; 54], String> {
    let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != 54 {
        return Err(format!("esperava 54 simbolos, recebi {}", chars.len()));
    }
    let centers = [chars[4], chars[13], chars[22], chars[31], chars[40], chars[49]];
    for (i, &c) in centers.iter().enumerate() {
        if c == UNKNOWN {
            return Err(format!("o centro de {} precisa estar pintado", FACE_NAMES[i]));
        }
        for j in (i + 1)..6 {
            if c == centers[j] {
                return Err(format!(
                    "os centros de {} e {} tem a mesma cor",
                    FACE_NAMES[i], FACE_NAMES[j]
                ));
            }
        }
    }
    let mut out = [None; 54];
    for (i, &c) in chars.iter().enumerate() {
        if c == UNKNOWN {
            continue;
        }
        match centers.iter().position(|&x| x == c) {
            Some(f) => out[i] = Some(f),
            None => {
                return Err(format!(
                    "o simbolo '{c}' na posicao {i} nao corresponde a nenhum centro"
                ))
            }
        }
    }
    Ok(out)
}

/// Cores (indices de face 0..6) que podem entrar na posicao `pos` mantendo o
/// cubo completavel. Centros retornam apenas a propria cor.
pub fn allowed_colors(input: &str, pos: usize) -> Result<Vec<usize>, String> {
    let f = parse_partial(input)?;
    if pos >= 54 {
        return Err("posicao fora da planificacao".into());
    }
    if pos % 9 == 4 {
        return Ok(vec![f[pos].expect("centro sempre pintado")]);
    }
    let mut out = Vec::new();
    for c in 0..6 {
        let mut g = f;
        g[pos] = Some(c);
        if feasible(&g) {
            out.push(c);
        }
    }
    Ok(out)
}

/// Existe um completamento valido deste parcial?
pub fn feasible(f: &[Option<usize>; 54]) -> bool {
    // ---- candidatos por encaixe de canto: (peca, orientacao; -1 = livre) ----
    let mut cand_c: Vec<Vec<(u8, i8)>> = Vec::with_capacity(8);
    let mut free_corner = false;
    for slot in 0..8 {
        let painted = CORNER_FACELET[slot].iter().any(|&p| f[p].is_some());
        if !painted {
            free_corner = true;
        }
        let mut v = Vec::new();
        for piece in 0..8 {
            for o in 0..3u8 {
                let ok = (0..3).all(|k| {
                    match f[CORNER_FACELET[slot][(k + o as usize) % 3]] {
                        Some(col) => col == CORNER_COLOR[piece][k],
                        None => true,
                    }
                });
                if ok {
                    if painted {
                        v.push((piece as u8, o as i8));
                    } else {
                        v.push((piece as u8, -1));
                        break; // sem adesivo pintado a orientacao e livre
                    }
                }
            }
        }
        if v.is_empty() {
            return false;
        }
        cand_c.push(v);
    }

    // ---- candidatos por encaixe de aresta --------------------------------
    let mut cand_e: Vec<Vec<(u8, i8)>> = Vec::with_capacity(12);
    let mut free_edge = false;
    for slot in 0..12 {
        let painted = EDGE_FACELET[slot].iter().any(|&p| f[p].is_some());
        if !painted {
            free_edge = true;
        }
        let mut v = Vec::new();
        for piece in 0..12 {
            for o in 0..2u8 {
                let ok = (0..2).all(|k| {
                    match f[EDGE_FACELET[slot][(k + o as usize) % 2]] {
                        Some(col) => col == EDGE_COLOR[piece][k],
                        None => true,
                    }
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
        cand_e.push(v);
    }

    // ---- paridades alcancaveis (ja filtradas pela soma de orientacoes) ----
    let sc = achievable(&cand_c, free_corner, 3);
    if !sc[0] && !sc[1] {
        return false;
    }
    let se = achievable(&cand_e, free_edge, 2);
    (sc[0] && se[0]) || (sc[1] && se[1])
}

/// Quais paridades de permutacao sao alcancaveis por emparelhamentos perfeitos
/// que respeitam a soma de orientacoes (quando ela esta toda forcada).
fn achievable(cand: &[Vec<(u8, i8)>], has_free: bool, modb: i32) -> [bool; 2] {
    let n = cand.len();
    // encaixes mais restritos primeiro (fail-first)
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&s| cand[s].len());

    struct S<'a> {
        cand: &'a [Vec<(u8, i8)>],
        order: &'a [usize],
        n: usize,
        has_free: bool,
        modb: i32,
        used: u16,
        perm: [u8; 12],
        oris: [i8; 12],
        out: [bool; 2],
        nodes: u32,
    }

    fn dfs(s: &mut S, k: usize) -> bool {
        // true = ja achamos as duas paridades, pode parar tudo
        s.nodes += 1;
        if s.nodes > 500_000 {
            // inatingivel na pratica; por seguranca, nao bloquear o usuario
            s.out = [true, true];
            return true;
        }
        if k == s.n {
            if !s.has_free {
                let sum: i32 = s.oris[..s.n].iter().map(|&o| o as i32).sum();
                if sum % s.modb != 0 {
                    return false;
                }
            }
            let mut par = 0u8;
            for i in 0..s.n {
                for j in (i + 1)..s.n {
                    if s.perm[j] < s.perm[i] {
                        par ^= 1;
                    }
                }
            }
            s.out[par as usize] = true;
            return s.out[0] && s.out[1];
        }
        let slot = s.order[k];
        for idx in 0..s.cand[slot].len() {
            let (p, o) = s.cand[slot][idx];
            if s.used & (1 << p) != 0 {
                continue;
            }
            s.used |= 1 << p;
            s.perm[slot] = p;
            s.oris[slot] = o.max(0);
            let stop = dfs(s, k + 1);
            s.used &= !(1 << p);
            if stop {
                return true;
            }
        }
        false
    }

    let mut s = S {
        cand,
        order: &order,
        n,
        has_free,
        modb,
        used: 0,
        perm: [0; 12],
        oris: [0; 12],
        out: [false; 2],
        nodes: 0,
    };
    dfs(&mut s, 0);
    s.out
}
