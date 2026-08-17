//! Tabelas de movimento e de poda (pruning). Geradas em paralelo no boot (~1s).

use crate::coord::*;
use crate::cube::*;
use crate::sym::BigP1;

pub struct Tables {
    /// Tabela grande da fase 1 (distancia exata, ~140 MB), quando habilitada.
    pub big: Option<BigP1>,

    pub mc: [CubieCube; N_MOVES],

    // Tabelas de movimento
    pub twist_move: Vec<u16>, // [twist][18]
    pub flip_move: Vec<u16>,  // [flip][18]
    pub slice_move: Vec<u16>, // [slice][18]
    pub cperm_move: Vec<u16>, // [cperm][10]
    pub uperm_move: Vec<u16>, // [uperm][10]
    pub sperm_move: Vec<u8>,  // [sperm][10]

    // Tabelas de poda (distancia minima ate o objetivo da fase)
    pub prun_twist: Vec<u8>, // [slice * N_TWIST + twist]
    pub prun_flip: Vec<u8>,  // [slice * N_FLIP  + flip]
    pub prun_tf: Vec<u8>,    // [twist * N_FLIP  + flip]
    pub prun_cperm: Vec<u8>, // [cperm * N_SPERM + sperm]
    pub prun_uperm: Vec<u8>, // [uperm * N_SPERM + sperm]
}

impl Tables {
    #[inline(always)]
    pub fn prun1(&self, twist: u16, flip: u16, slice: u16) -> u8 {
        if let Some(big) = &self.big {
            return big.h(twist, flip, slice); // distancia exata
        }
        let a = self.prun_twist[slice as usize * N_TWIST + twist as usize];
        let b = self.prun_flip[slice as usize * N_FLIP + flip as usize];
        let c = self.prun_tf[twist as usize * N_FLIP + flip as usize];
        a.max(b).max(c)
    }

    #[inline(always)]
    pub fn prun2(&self, cperm: u16, uperm: u16, sperm: u8) -> u8 {
        let a = self.prun_cperm[cperm as usize * N_SPERM + sperm as usize];
        let b = self.prun_uperm[uperm as usize * N_SPERM + sperm as usize];
        if a > b {
            a
        } else {
            b
        }
    }

    pub fn build() -> Tables {
        let mc = move_cubes();

        let (twist_move, flip_move, slice_move, cperm_move, uperm_move, sperm_move) =
            std::thread::scope(|s| {
                let a = s.spawn(|| build_twist_move(&mc));
                let b = s.spawn(|| build_flip_move(&mc));
                let c = s.spawn(|| build_slice_move(&mc));
                let d = s.spawn(|| build_cperm_move(&mc));
                let e = s.spawn(|| build_uperm_move(&mc));
                let f = s.spawn(|| build_sperm_move(&mc));
                (
                    a.join().unwrap(),
                    b.join().unwrap(),
                    c.join().unwrap(),
                    d.join().unwrap(),
                    e.join().unwrap(),
                    f.join().unwrap(),
                )
            });

        let (prun_twist, prun_flip, prun_tf, prun_cperm, prun_uperm) = std::thread::scope(|s| {
            let (sm, tm, fm) = (&slice_move, &twist_move, &flip_move);
            let (cm, um, spm) = (&cperm_move, &uperm_move, &sperm_move);
            let a = s.spawn(move || {
                build_prun(
                    N_SLICE,
                    N_TWIST,
                    N_MOVES,
                    |x, m| sm[x * N_MOVES + m] as usize,
                    |x, m| tm[x * N_MOVES + m] as usize,
                )
            });
            let b = s.spawn(move || {
                build_prun(
                    N_SLICE,
                    N_FLIP,
                    N_MOVES,
                    |x, m| sm[x * N_MOVES + m] as usize,
                    |x, m| fm[x * N_MOVES + m] as usize,
                )
            });
            let e = s.spawn(move || {
                build_prun(
                    N_TWIST,
                    N_FLIP,
                    N_MOVES,
                    |x, m| tm[x * N_MOVES + m] as usize,
                    |x, m| fm[x * N_MOVES + m] as usize,
                )
            });
            let c = s.spawn(move || {
                build_prun(
                    N_CPERM,
                    N_SPERM,
                    N_P2_MOVES,
                    |x, m| cm[x * N_P2_MOVES + m] as usize,
                    |x, m| spm[x * N_P2_MOVES + m] as usize,
                )
            });
            let d = s.spawn(move || {
                build_prun(
                    N_UPERM,
                    N_SPERM,
                    N_P2_MOVES,
                    |x, m| um[x * N_P2_MOVES + m] as usize,
                    |x, m| spm[x * N_P2_MOVES + m] as usize,
                )
            });
            (
                a.join().unwrap(),
                b.join().unwrap(),
                e.join().unwrap(),
                c.join().unwrap(),
                d.join().unwrap(),
            )
        });

        Tables {
            big: None,
            mc,
            twist_move,
            flip_move,
            slice_move,
            cperm_move,
            uperm_move,
            sperm_move,
            prun_twist,
            prun_flip,
            prun_tf,
            prun_cperm,
            prun_uperm,
        }
    }
}

// ---------------------------------------------------------------------------
// Tabelas de movimento
// ---------------------------------------------------------------------------

fn build_twist_move(mc: &[CubieCube; N_MOVES]) -> Vec<u16> {
    let mut t = vec![0u16; N_TWIST * N_MOVES];
    let mut c = SOLVED;
    for i in 0..N_TWIST {
        set_twist(i as u16, &mut c.co);
        for f in 0..6 {
            let mut d = c;
            for p in 0..3 {
                d = d.multiply(&mc[f * 3]);
                t[i * N_MOVES + f * 3 + p] = get_twist(&d.co);
            }
        }
    }
    t
}

fn build_flip_move(mc: &[CubieCube; N_MOVES]) -> Vec<u16> {
    let mut t = vec![0u16; N_FLIP * N_MOVES];
    let mut c = SOLVED;
    for i in 0..N_FLIP {
        set_flip(i as u16, &mut c.eo);
        for f in 0..6 {
            let mut d = c;
            for p in 0..3 {
                d = d.multiply(&mc[f * 3]);
                t[i * N_MOVES + f * 3 + p] = get_flip(&d.eo);
            }
        }
    }
    t
}

fn build_slice_move(mc: &[CubieCube; N_MOVES]) -> Vec<u16> {
    let mut t = vec![0u16; N_SLICE * N_MOVES];
    let mut c = SOLVED;
    for i in 0..N_SLICE {
        set_slice(i as u16, &mut c.ep);
        for f in 0..6 {
            let mut d = c;
            for p in 0..3 {
                d = d.multiply(&mc[f * 3]);
                t[i * N_MOVES + f * 3 + p] = get_slice(&d.ep);
            }
        }
    }
    t
}

fn build_cperm_move(mc: &[CubieCube; N_MOVES]) -> Vec<u16> {
    let mut t = vec![0u16; N_CPERM * N_P2_MOVES];
    let mut c = SOLVED;
    for i in 0..N_CPERM {
        perm_from_index(i as u32, 8, &mut c.cp);
        for j in 0..N_P2_MOVES {
            let d = c.multiply(&mc[P2_MOVES[j] as usize]);
            t[i * N_P2_MOVES + j] = get_cperm(&d.cp);
        }
    }
    t
}

fn build_uperm_move(mc: &[CubieCube; N_MOVES]) -> Vec<u16> {
    let mut t = vec![0u16; N_UPERM * N_P2_MOVES];
    let mut c = SOLVED;
    let mut p8 = [0u8; 8];
    for i in 0..N_UPERM {
        perm_from_index(i as u32, 8, &mut p8);
        c.ep[0..8].copy_from_slice(&p8);
        for k in 8..12 {
            c.ep[k] = k as u8;
        }
        for j in 0..N_P2_MOVES {
            let d = c.multiply(&mc[P2_MOVES[j] as usize]);
            t[i * N_P2_MOVES + j] = get_uperm(&d.ep);
        }
    }
    t
}

fn build_sperm_move(mc: &[CubieCube; N_MOVES]) -> Vec<u8> {
    let mut t = vec![0u8; N_SPERM * N_P2_MOVES];
    let mut c = SOLVED;
    let mut p4 = [0u8; 4];
    for i in 0..N_SPERM {
        perm_from_index(i as u32, 4, &mut p4);
        for k in 0..8 {
            c.ep[k] = k as u8;
        }
        for k in 0..4 {
            c.ep[8 + k] = p4[k] + 8;
        }
        for j in 0..N_P2_MOVES {
            let d = c.multiply(&mc[P2_MOVES[j] as usize]);
            t[i * N_P2_MOVES + j] = get_sperm(&d.ep);
        }
    }
    t
}

// ---------------------------------------------------------------------------
// Poda: BFS a partir do estado objetivo (indice 0) no produto de duas coordenadas
// ---------------------------------------------------------------------------

fn build_prun<FA, FB>(n_a: usize, n_b: usize, n_moves: usize, next_a: FA, next_b: FB) -> Vec<u8>
where
    FA: Fn(usize, usize) -> usize,
    FB: Fn(usize, usize) -> usize,
{
    let total = n_a * n_b;
    let mut dist = vec![255u8; total];
    dist[0] = 0;
    let mut frontier: Vec<u32> = vec![0];
    let mut depth: u8 = 0;
    while !frontier.is_empty() {
        let mut next: Vec<u32> = Vec::with_capacity(frontier.len() * 3);
        for &idx in &frontier {
            let a = idx as usize / n_b;
            let b = idx as usize % n_b;
            for m in 0..n_moves {
                let ni = next_a(a, m) * n_b + next_b(b, m);
                if dist[ni] == 255 {
                    dist[ni] = depth + 1;
                    next.push(ni as u32);
                }
            }
        }
        frontier = next;
        depth += 1;
    }
    dist
}
