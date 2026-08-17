//! Conversao entre a "planificacao" de 54 adesivos e o estado cubie.
//!
//! Ordem dos adesivos: U(0..8) R(9..17) F(18..26) D(27..35) L(36..44) B(45..53)
//! Dentro de cada face, leitura em linhas (esquerda->direita, cima->baixo).

use crate::cube::{CubieCube, SOLVED};

pub const FACE_CHARS: [u8; 6] = [b'U', b'R', b'F', b'D', b'L', b'B'];
pub const FACE_NAMES: [&str; 6] = ["U (topo)", "R (direita)", "F (frente)", "D (baixo)", "L (esquerda)", "B (tras)"];

/// Adesivos de cada canto, na ordem da sua "cor 1, cor 2, cor 3".
pub const CORNER_FACELET: [[usize; 3]; 8] = [
    [8, 9, 20],   // URF
    [6, 18, 38],  // UFL
    [0, 36, 47],  // ULB
    [2, 45, 11],  // UBR
    [29, 26, 15], // DFR
    [27, 44, 24], // DLF
    [33, 53, 42], // DBL
    [35, 17, 51], // DRB
];

/// Faces (cores) de cada canto, mesma ordem de CORNER_FACELET.
pub const CORNER_COLOR: [[usize; 3]; 8] = [
    [0, 1, 2], // URF
    [0, 2, 4], // UFL
    [0, 4, 5], // ULB
    [0, 5, 1], // UBR
    [3, 2, 1], // DFR
    [3, 4, 2], // DLF
    [3, 5, 4], // DBL
    [3, 1, 5], // DRB
];

pub const EDGE_FACELET: [[usize; 2]; 12] = [
    [5, 10],  // UR
    [7, 19],  // UF
    [3, 37],  // UL
    [1, 46],  // UB
    [32, 16], // DR
    [28, 25], // DF
    [30, 43], // DL
    [34, 52], // DB
    [23, 12], // FR
    [21, 41], // FL
    [50, 39], // BL
    [48, 14], // BR
];

pub const EDGE_COLOR: [[usize; 2]; 12] = [
    [0, 1], // UR
    [0, 2], // UF
    [0, 4], // UL
    [0, 5], // UB
    [3, 1], // DR
    [3, 2], // DF
    [3, 4], // DL
    [3, 5], // DB
    [2, 1], // FR
    [2, 4], // FL
    [5, 4], // BL
    [5, 1], // BR
];

/// Posicao (linha, coluna) de cada adesivo na planificacao 12x9, util para mensagens de erro.
pub fn facelet_label(i: usize) -> String {
    format!("{}{}", FACE_CHARS[i / 9] as char, i % 9 + 1)
}

/// Normaliza uma entrada de 54 caracteres quaisquer (cores) para a string U/R/F/D/L/B,
/// usando os centros como referencia de orientacao.
pub fn normalize(input: &str) -> Result<[usize; 54], String> {
    let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != 54 {
        return Err(format!(
            "esperava 54 adesivos, recebi {} (todos os quadradinhos precisam estar pintados)",
            chars.len()
        ));
    }

    // Os 6 centros definem qual cor pertence a qual face.
    let centers = [chars[4], chars[13], chars[22], chars[31], chars[40], chars[49]];
    for i in 0..6 {
        for j in (i + 1)..6 {
            if centers[i] == centers[j] {
                return Err(format!(
                    "os centros de {} e {} tem a mesma cor - cada face precisa de um centro diferente",
                    FACE_NAMES[i], FACE_NAMES[j]
                ));
            }
        }
    }

    let mut out = [0usize; 54];
    let mut count = [0usize; 6];
    for (i, c) in chars.iter().enumerate() {
        match centers.iter().position(|x| x == c) {
            Some(f) => {
                out[i] = f;
                count[f] += 1;
            }
            None => {
                return Err(format!(
                    "o adesivo {} tem uma cor que nao corresponde a nenhum centro",
                    facelet_label(i)
                ))
            }
        }
    }
    for f in 0..6 {
        if count[f] != 9 {
            return Err(format!(
                "a cor da face {} aparece {} vezes (deveriam ser exatamente 9)",
                FACE_NAMES[f], count[f]
            ));
        }
    }
    Ok(out)
}

/// Planificacao -> estado cubie, com validacao completa.
pub fn to_cubie(input: &str) -> Result<CubieCube, String> {
    let f = normalize(input)?;
    let mut c = SOLVED;

    // Cantos
    let mut used = [false; 8];
    for i in 0..8 {
        // Acha a orientacao: quantas rotacoes ate a etiqueta U ou D.
        let mut ori = 0;
        while ori < 3 {
            let col = f[CORNER_FACELET[i][ori]];
            if col == 0 || col == 3 {
                break;
            }
            ori += 1;
        }
        if ori == 3 {
            return Err(format!(
                "o canto {} nao tem nenhum adesivo da cor do topo nem do fundo",
                corner_name(i)
            ));
        }
        let c1 = f[CORNER_FACELET[i][(ori + 1) % 3]];
        let c2 = f[CORNER_FACELET[i][(ori + 2) % 3]];
        let mut found = None;
        for j in 0..8 {
            if c1 == CORNER_COLOR[j][1] && c2 == CORNER_COLOR[j][2] {
                found = Some(j);
                break;
            }
        }
        match found {
            Some(j) => {
                if used[j] {
                    return Err(format!(
                        "o canto {} esta repetido - confira as cores",
                        corner_name(j)
                    ));
                }
                used[j] = true;
                c.cp[i] = j as u8;
                c.co[i] = ori as u8;
            }
            None => {
                return Err(format!(
                    "a combinacao de cores do canto {} nao existe em um cubo real",
                    corner_name(i)
                ))
            }
        }
    }

    // Arestas
    let mut used = [false; 12];
    for i in 0..12 {
        let a = f[EDGE_FACELET[i][0]];
        let b = f[EDGE_FACELET[i][1]];
        let mut found = None;
        for j in 0..12 {
            if a == EDGE_COLOR[j][0] && b == EDGE_COLOR[j][1] {
                found = Some((j, 0u8));
                break;
            }
            if a == EDGE_COLOR[j][1] && b == EDGE_COLOR[j][0] {
                found = Some((j, 1u8));
                break;
            }
        }
        match found {
            Some((j, ori)) => {
                if used[j] {
                    return Err(format!(
                        "a aresta {} esta repetida - confira as cores",
                        edge_name(j)
                    ));
                }
                used[j] = true;
                c.ep[i] = j as u8;
                c.eo[i] = ori;
            }
            None => {
                return Err(format!(
                    "a combinacao de cores da aresta {} nao existe em um cubo real",
                    edge_name(i)
                ))
            }
        }
    }

    c.verify()?;
    Ok(c)
}

/// Estado cubie -> planificacao de 54 letras U/R/F/D/L/B.
pub fn to_facelets(c: &CubieCube) -> String {
    let mut f = [b'?'; 54];
    for i in 0..6 {
        f[i * 9 + 4] = FACE_CHARS[i];
    }
    for i in 0..8 {
        let j = c.cp[i] as usize;
        let ori = c.co[i] as usize;
        for k in 0..3 {
            f[CORNER_FACELET[i][(k + ori) % 3]] = FACE_CHARS[CORNER_COLOR[j][k]];
        }
    }
    for i in 0..12 {
        let j = c.ep[i] as usize;
        let ori = c.eo[i] as usize;
        for k in 0..2 {
            f[EDGE_FACELET[i][(k + ori) % 2]] = FACE_CHARS[EDGE_COLOR[j][k]];
        }
    }
    String::from_utf8(f.to_vec()).unwrap()
}

// ---------------------------------------------------------------------------
// Rotacoes do cubo inteiro em torno do eixo URF-DBL
//
// A fase 1 do Kociemba e definida em relacao ao eixo U/D. Girando o cubo inteiro
// antes de buscar, o mesmo algoritmo passa a usar o eixo R/L ou F/B, o que da
// tres buscas genuinamente diferentes para a mesma posicao.
// ---------------------------------------------------------------------------

/// pi[f] = face para onde a face f vai. 0 = sem rotacao, 1 = 120 graus, 2 = 240 graus.
pub const ROT_PI: [[usize; 6]; 3] = [
    [0, 1, 2, 3, 4, 5], // identidade
    [1, 2, 0, 4, 5, 3], // U->R, R->F, F->U, D->L, L->B, B->D
    [2, 0, 1, 5, 3, 4], // U->F, F->R, R->U, D->B, B->L, L->D
];

/// Para onde cada posicao de adesivo vai sob a rotacao `pi`.
pub fn rotation_perm(pi: &[usize; 6]) -> [usize; 54] {
    let mut rot = [usize::MAX; 54];
    for f in 0..6 {
        rot[f * 9 + 4] = pi[f] * 9 + 4;
    }
    for i in 0..8 {
        let mut done = false;
        for ii in 0..8 {
            for o in 0..3 {
                if (0..3).all(|k| CORNER_COLOR[ii][(k + o) % 3] == pi[CORNER_COLOR[i][k]]) {
                    for k in 0..3 {
                        rot[CORNER_FACELET[i][k]] = CORNER_FACELET[ii][(k + o) % 3];
                    }
                    done = true;
                    break;
                }
            }
            if done {
                break;
            }
        }
        debug_assert!(done, "rotacao invalida para o canto {i}");
    }
    for i in 0..12 {
        let mut done = false;
        for ii in 0..12 {
            for o in 0..2 {
                if (0..2).all(|k| EDGE_COLOR[ii][(k + o) % 2] == pi[EDGE_COLOR[i][k]]) {
                    for k in 0..2 {
                        rot[EDGE_FACELET[i][k]] = EDGE_FACELET[ii][(k + o) % 2];
                    }
                    done = true;
                    break;
                }
            }
            if done {
                break;
            }
        }
        debug_assert!(done, "rotacao invalida para a aresta {i}");
    }
    rot
}

/// Gira o cubo inteiro. O estado resultante e o mesmo cubo visto de outro angulo.
pub fn rotate_cube(c: &CubieCube, pi: &[usize; 6], rot: &[usize; 54]) -> CubieCube {
    let src = to_facelets(c).into_bytes();
    let mut out = [b'?'; 54];
    for p in 0..54 {
        let f = FACE_CHARS.iter().position(|&x| x == src[p]).unwrap();
        out[rot[p]] = FACE_CHARS[pi[f]];
    }
    to_cubie(std::str::from_utf8(&out).unwrap()).expect("rotacao de um cubo valido e valida")
}

/// Traducao de faces do referencial girado de volta para o original.
pub fn inverse_face_map(pi: &[usize; 6]) -> [u8; 6] {
    let mut inv = [0u8; 6];
    for f in 0..6 {
        inv[pi[f]] = f as u8;
    }
    inv
}

pub fn corner_name(i: usize) -> &'static str {
    ["URF", "UFL", "ULB", "UBR", "DFR", "DLF", "DBL", "DRB"][i]
}

pub fn edge_name(i: usize) -> &'static str {
    ["UR", "UF", "UL", "UB", "DR", "DF", "DL", "DB", "FR", "FL", "BL", "BR"][i]
}
