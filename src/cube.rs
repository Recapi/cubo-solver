//! Representacao "cubie" do cubo 3x3 e os 18 movimentos basicos.
//!
//! Convencoes (identicas as do Kociemba):
//!   cantos:  URF UFL ULB UBR DFR DLF DBL DRB  -> 0..7
//!   arestas: UR UF UL UB DR DF DL DB FR FL BL BR -> 0..11
//!   faces:   U=0 R=1 F=2 D=3 L=4 B=5
//!   moves:   face*3 + potencia, onde potencia 0 = 90deg horario, 1 = 180deg, 2 = 90deg anti-horario

pub const N_MOVES: usize = 18;

pub const MOVE_NAMES: [&str; N_MOVES] = [
    "U", "U2", "U'", "R", "R2", "R'", "F", "F2", "F'", "D", "D2", "D'", "L", "L2", "L'", "B", "B2",
    "B'",
];

/// Movimentos permitidos na fase 2 (subgrupo G1 = <U, D, R2, L2, F2, B2>).
pub const P2_MOVES: [u8; 10] = [0, 1, 2, 4, 7, 9, 10, 11, 13, 16];
pub const N_P2_MOVES: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CubieCube {
    pub cp: [u8; 8],
    pub co: [u8; 8],
    pub ep: [u8; 12],
    pub eo: [u8; 12],
}

pub const SOLVED: CubieCube = CubieCube {
    cp: [0, 1, 2, 3, 4, 5, 6, 7],
    co: [0; 8],
    ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    eo: [0; 12],
};

impl CubieCube {
    /// self * b  (aplicar b depois de self).
    #[inline]
    pub fn multiply(&self, b: &CubieCube) -> CubieCube {
        let mut r = SOLVED;
        for i in 0..8 {
            let k = b.cp[i] as usize;
            r.cp[i] = self.cp[k];
            r.co[i] = (self.co[k] + b.co[i]) % 3;
        }
        for i in 0..12 {
            let k = b.ep[i] as usize;
            r.ep[i] = self.ep[k];
            r.eo[i] = (self.eo[k] + b.eo[i]) % 2;
        }
        r
    }

    pub fn is_solved(&self) -> bool {
        *self == SOLVED
    }

    /// Estado inverso: se S resolve `self.inverse()`, entao S invertida resolve `self`.
    pub fn inverse(&self) -> CubieCube {
        let mut r = SOLVED;
        for i in 0..8 {
            r.cp[self.cp[i] as usize] = i as u8;
        }
        for i in 0..8 {
            r.co[i] = (3 - self.co[r.cp[i] as usize]) % 3;
        }
        for i in 0..12 {
            r.ep[self.ep[i] as usize] = i as u8;
        }
        for i in 0..12 {
            r.eo[i] = self.eo[r.ep[i] as usize];
        }
        r
    }

    /// Verifica se o estado e fisicamente possivel (paridades e orientacoes).
    pub fn verify(&self) -> Result<(), String> {
        let mut seen = [false; 12];
        for i in 0..12 {
            let e = self.ep[i] as usize;
            if e >= 12 || seen[e] {
                return Err("uma aresta aparece duas vezes no cubo".into());
            }
            seen[e] = true;
        }
        let mut seen = [false; 8];
        for i in 0..8 {
            let c = self.cp[i] as usize;
            if c >= 8 || seen[c] {
                return Err("um canto aparece duas vezes no cubo".into());
            }
            seen[c] = true;
        }

        let flip: u32 = self.eo.iter().map(|&x| x as u32).sum();
        if flip % 2 != 0 {
            return Err("uma aresta esta invertida (orientacao das arestas impossivel)".into());
        }
        let twist: u32 = self.co.iter().map(|&x| x as u32).sum();
        if twist % 3 != 0 {
            return Err("um canto esta torcido (orientacao dos cantos impossivel)".into());
        }
        if perm_parity(&self.ep) != perm_parity(&self.cp) {
            return Err("paridade invalida: duas pecas precisam ser trocadas".into());
        }
        Ok(())
    }
}

pub fn perm_parity(p: &[u8]) -> u8 {
    let mut s = 0u32;
    for i in 0..p.len() {
        for j in (i + 1)..p.len() {
            if p[j] < p[i] {
                s += 1;
            }
        }
    }
    (s % 2) as u8
}

// ---------------------------------------------------------------------------
// Os 6 movimentos basicos de 90 graus
// ---------------------------------------------------------------------------

const MOVE_U: CubieCube = CubieCube {
    cp: [3, 0, 1, 2, 4, 5, 6, 7],
    co: [0, 0, 0, 0, 0, 0, 0, 0],
    ep: [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
    eo: [0; 12],
};

const MOVE_R: CubieCube = CubieCube {
    cp: [4, 1, 2, 0, 7, 5, 6, 3],
    co: [2, 0, 0, 1, 1, 0, 0, 2],
    ep: [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],
    eo: [0; 12],
};

const MOVE_F: CubieCube = CubieCube {
    cp: [1, 5, 2, 3, 0, 4, 6, 7],
    co: [1, 2, 0, 0, 2, 1, 0, 0],
    ep: [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],
    eo: [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],
};

const MOVE_D: CubieCube = CubieCube {
    cp: [0, 1, 2, 3, 5, 6, 7, 4],
    co: [0, 0, 0, 0, 0, 0, 0, 0],
    ep: [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],
    eo: [0; 12],
};

const MOVE_L: CubieCube = CubieCube {
    cp: [0, 2, 6, 3, 4, 1, 5, 7],
    co: [0, 1, 2, 0, 0, 2, 1, 0],
    ep: [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],
    eo: [0; 12],
};

const MOVE_B: CubieCube = CubieCube {
    cp: [0, 1, 3, 7, 4, 5, 2, 6],
    co: [0, 0, 1, 2, 0, 0, 2, 1],
    ep: [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],
    eo: [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
};

const BASIC: [CubieCube; 6] = [MOVE_U, MOVE_R, MOVE_F, MOVE_D, MOVE_L, MOVE_B];

/// Constroi os 18 cubos de movimento (X, X2, X').
pub fn move_cubes() -> [CubieCube; N_MOVES] {
    let mut mc = [SOLVED; N_MOVES];
    for f in 0..6 {
        let mut c = SOLVED;
        for p in 0..3 {
            c = c.multiply(&BASIC[f]);
            mc[f * 3 + p] = c;
        }
    }
    mc
}

/// Nome do movimento em notacao padrao.
pub fn move_name(m: u8) -> &'static str {
    MOVE_NAMES[m as usize]
}

#[inline]
pub fn move_face(m: u8) -> u8 {
    m / 3
}

/// Eixo do movimento: U/D = 0, R/L = 1, F/B = 2.
#[inline]
pub fn move_axis(m: u8) -> u8 {
    (m / 3) % 3
}

/// Movimento inverso: X <-> X', X2 <-> X2.
#[inline]
pub fn move_inverse(m: u8) -> u8 {
    (m / 3) * 3 + (2 - m % 3)
}
