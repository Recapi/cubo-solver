//! Conversao entre estado "cubie" e coordenadas numericas usadas na busca.
//!
//! Fase 1: twist (2187) x flip (2048) x slice (495)
//! Fase 2: cperm (40320) x uperm (40320) x sperm (24)

pub const N_TWIST: usize = 2187; // 3^7
pub const N_FLIP: usize = 2048; // 2^11
pub const N_SLICE: usize = 495; // C(12,4)
pub const N_CPERM: usize = 40320; // 8!
pub const N_UPERM: usize = 40320; // 8!
pub const N_SPERM: usize = 24; // 4!

/// Coeficiente binomial para n,k <= 12.
pub fn cnk(n: u32, k: u32) -> u32 {
    if k > n {
        return 0;
    }
    let mut r: u64 = 1;
    for i in 0..k {
        r = r * (n - i) as u64 / (i + 1) as u64;
    }
    r as u32
}

// ---------------------------------------------------------------------------
// Orientacao dos cantos (twist)
// ---------------------------------------------------------------------------

pub fn get_twist(co: &[u8; 8]) -> u16 {
    let mut r: u16 = 0;
    for i in 0..7 {
        r = 3 * r + co[i] as u16;
    }
    r
}

pub fn set_twist(mut t: u16, co: &mut [u8; 8]) {
    let mut s: u16 = 0;
    for i in (0..7).rev() {
        co[i] = (t % 3) as u8;
        s += co[i] as u16;
        t /= 3;
    }
    co[7] = ((3 - s % 3) % 3) as u8;
}

// ---------------------------------------------------------------------------
// Orientacao das arestas (flip)
// ---------------------------------------------------------------------------

pub fn get_flip(eo: &[u8; 12]) -> u16 {
    let mut r: u16 = 0;
    for i in 0..11 {
        r = 2 * r + eo[i] as u16;
    }
    r
}

pub fn set_flip(mut f: u16, eo: &mut [u8; 12]) {
    let mut s: u16 = 0;
    for i in (0..11).rev() {
        eo[i] = (f % 2) as u8;
        s += eo[i] as u16;
        f /= 2;
    }
    eo[11] = ((2 - s % 2) % 2) as u8;
}

// ---------------------------------------------------------------------------
// Posicao (nao ordenada) das 4 arestas da fatia do meio: FR FL BL BR
// slice == 0  <=>  as 4 arestas estao nas posicoes 8..11
// ---------------------------------------------------------------------------

pub fn get_slice(ep: &[u8; 12]) -> u16 {
    let mut a: u32 = 0;
    let mut x: u32 = 0;
    for j in (0..12).rev() {
        if ep[j] >= 8 {
            a += cnk(11 - j as u32, x + 1);
            x += 1;
        }
    }
    a as u16
}

pub fn set_slice(idx: u16, ep: &mut [u8; 12]) {
    let mut a = idx as u32;
    let mut mark = [false; 12];
    // Sistema numerico combinatorio: a = C(q1,1)+C(q2,2)+C(q3,3)+C(q4,4), q1<q2<q3<q4.
    for k in (1..=4u32).rev() {
        let mut q = 11u32;
        while cnk(q, k) > a {
            q -= 1;
        }
        a -= cnk(q, k);
        mark[(11 - q) as usize] = true;
    }
    let mut slice_val = 8u8;
    let mut other_val = 0u8;
    for j in 0..12 {
        if mark[j] {
            ep[j] = slice_val;
            slice_val += 1;
        } else {
            ep[j] = other_val;
            other_val += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Indice de permutacao (codigo de Lehmer). Identidade -> 0.
// ---------------------------------------------------------------------------

pub fn perm_index(p: &[u8]) -> u32 {
    let n = p.len();
    let mut idx: u32 = 0;
    for i in 0..n {
        let mut c: u32 = 0;
        for j in (i + 1)..n {
            if p[j] < p[i] {
                c += 1;
            }
        }
        idx = idx * (n - i) as u32 + c;
    }
    idx
}

pub fn perm_from_index(mut idx: u32, n: usize, out: &mut [u8]) {
    let mut c = [0usize; 12];
    for i in (0..n).rev() {
        let base = (n - i) as u32;
        c[i] = (idx % base) as usize;
        idx /= base;
    }
    let mut avail = [0u8; 12];
    for i in 0..n {
        avail[i] = i as u8;
    }
    let mut len = n;
    for i in 0..n {
        let k = c[i];
        out[i] = avail[k];
        for j in k..(len - 1) {
            avail[j] = avail[j + 1];
        }
        len -= 1;
    }
}

// ---------------------------------------------------------------------------
// Coordenadas da fase 2 lidas direto do estado cubie (o cubo ja esta em G1)
// ---------------------------------------------------------------------------

pub fn get_cperm(cp: &[u8; 8]) -> u16 {
    perm_index(cp) as u16
}

pub fn get_uperm(ep: &[u8; 12]) -> u16 {
    let mut q = [0u8; 8];
    q.copy_from_slice(&ep[0..8]);
    perm_index(&q) as u16
}

pub fn get_sperm(ep: &[u8; 12]) -> u8 {
    let q = [ep[8] - 8, ep[9] - 8, ep[10] - 8, ep[11] - 8];
    perm_index(&q) as u8
}
