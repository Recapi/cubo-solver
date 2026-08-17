//! Tabela de poda "grande" da fase 1, com reducao por simetria.
//!
//! As 16 simetrias do cubo que preservam o eixo U/D (4 rotacoes em torno de U
//! x meia-volta em torno de F x espelho esquerda-direita) mapeiam o subgrupo G1
//! nele mesmo e permutam os 18 movimentos entre si. Logo a distancia ate G1 e
//! invariante por conjugacao, e o espaco flip x slice (2048 x 495 = 1.013.760)
//! se reduz a 64.430 classes de equivalencia. Cruzando com twist (2187), a
//! tabela tem 64.430 x 2187 = 140.908.410 entradas de 1 byte (~140 MB) com a
//! DISTANCIA EXATA da fase 1 de qualquer estado.
//!
//! Para evitar a aritmetica de orientacao de canto espelhada (fonte classica de
//! bug), a conjugacao de cantos e feita no nivel da planificacao (to_facelets ->
//! permutacao de adesivos -> to_cubie), e a de arestas por multiplicacao direta
//! (orientacao mod 2 nao e afetada pelo espelho).

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use crate::coord::*;
use crate::cube::{CubieCube, N_P2_MOVES, SOLVED};
use crate::facelet::{rotation_perm, to_cubie, to_facelets, CORNER_FACELET, EDGE_FACELET, FACE_CHARS};
use crate::tables::Tables;

pub const N_SYM: usize = 16;
pub const N_RAW: usize = N_SLICE * N_FLIP; // 1.013.760
pub const N_CLASS: usize = 64430;
/// Classes das 40320 permutacoes de canto sob as 16 simetrias.
pub const N_CLASS2: usize = 2768;
const CACHE_VERSION: u32 = 1;

fn workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16)
}

// ---------------------------------------------------------------------------
// Simetrias como transformacoes da planificacao
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SymXform {
    /// perm[p] = posicao de destino do adesivo que esta em p.
    pub perm: [usize; 54],
    /// color[f] = nova letra da face f.
    pub color: [usize; 6],
}

fn compose(a: &SymXform, b: &SymXform) -> SymXform {
    let mut perm = [0usize; 54];
    let mut color = [0usize; 6];
    for p in 0..54 {
        perm[p] = b.perm[a.perm[p]];
    }
    for f in 0..6 {
        color[f] = b.color[a.color[f]];
    }
    SymXform { perm, color }
}

/// Aplica a transformacao a uma planificacao (letras U/R/F/D/L/B).
pub fn apply_xform(x: &SymXform, facelets: &str) -> String {
    let src = facelets.as_bytes();
    let mut out = [b'?'; 54];
    for p in 0..54 {
        let f = FACE_CHARS.iter().position(|&c| c == src[p]).unwrap();
        out[x.perm[p]] = FACE_CHARS[x.color[f]];
    }
    String::from_utf8(out.to_vec()).unwrap()
}

fn identity() -> SymXform {
    let mut perm = [0usize; 54];
    for p in 0..54 {
        perm[p] = p;
    }
    SymXform { perm, color: [0, 1, 2, 3, 4, 5] }
}

fn from_rotation(pi: [usize; 6]) -> SymXform {
    SymXform { perm: rotation_perm(&pi), color: pi }
}

/// Espelho esquerda-direita: cada face espelha suas colunas; L e R trocam.
fn mirror_lr() -> SymXform {
    let mut perm = [0usize; 54];
    for f in 0..6 {
        let df = match f {
            1 => 4, // R -> L
            4 => 1, // L -> R
            _ => f,
        };
        for r in 0..3 {
            for c in 0..3 {
                perm[f * 9 + r * 3 + c] = df * 9 + r * 3 + (2 - c);
            }
        }
    }
    SymXform { perm, color: [0, 4, 2, 3, 1, 5] }
}

/// As 16 simetrias do eixo U/D: y^i * f2^j * espelho^k.
pub fn symmetries() -> Vec<SymXform> {
    // y: rotacao do cubo inteiro em torno de U (conteudo de F vai para L).
    let y = from_rotation([0, 2, 4, 3, 5, 1]);
    // f2: meia-volta em torno do eixo F (U <-> D, R <-> L).
    let f2 = from_rotation([3, 4, 2, 0, 1, 5]);
    let m = mirror_lr();

    let mut out = Vec::with_capacity(N_SYM);
    let mut yi = identity();
    for _ in 0..4 {
        for j in 0..2 {
            for k in 0..2 {
                let mut s = yi.clone();
                if j == 1 {
                    s = compose(&s, &f2);
                }
                if k == 1 {
                    s = compose(&s, &m);
                }
                out.push(s);
            }
        }
        yi = compose(&yi, &y);
    }
    out
}

/// Conjugacao completa de um estado via planificacao (usada nos testes e no
/// twist_conj; devagar porem imune a convencoes de orientacao espelhada).
#[cfg_attr(not(test), allow(dead_code))]
pub fn conj_state(c: &CubieCube, x: &SymXform) -> CubieCube {
    to_cubie(&apply_xform(x, &to_facelets(c))).expect("conjugado de estado valido e valido")
}

// ---------------------------------------------------------------------------
// Arestas: representacao minima para conjugar flip x slice rapido
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct EdgeCube {
    pub ep: [u8; 12],
    pub eo: [u8; 12],
}

pub const EDGE_ID: EdgeCube =
    EdgeCube { ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], eo: [0; 12] };

pub fn edge_mult(a: &EdgeCube, b: &EdgeCube) -> EdgeCube {
    let mut r = EDGE_ID;
    for i in 0..12 {
        let k = b.ep[i] as usize;
        r.ep[i] = a.ep[k];
        r.eo[i] = (a.eo[k] + b.eo[i]) % 2;
    }
    r
}

pub fn edge_inverse(a: &EdgeCube) -> EdgeCube {
    let mut r = EDGE_ID;
    for i in 0..12 {
        r.ep[a.ep[i] as usize] = i as u8;
    }
    for i in 0..12 {
        r.eo[i] = a.eo[r.ep[i] as usize];
    }
    r
}

/// Extrai a acao da simetria sobre as arestas a partir da permutacao de adesivos.
pub fn edge_cube_of(x: &SymXform) -> EdgeCube {
    let mut s = EDGE_ID;
    for j in 0..12 {
        let a = x.perm[EDGE_FACELET[j][0]];
        let b = x.perm[EDGE_FACELET[j][1]];
        let mut found = false;
        for k in 0..12 {
            if EDGE_FACELET[k][0] == a && EDGE_FACELET[k][1] == b {
                s.ep[k] = j as u8;
                s.eo[k] = 0;
                found = true;
                break;
            }
            if EDGE_FACELET[k][0] == b && EDGE_FACELET[k][1] == a {
                s.ep[k] = j as u8;
                s.eo[k] = 1;
                found = true;
                break;
            }
        }
        debug_assert!(found, "simetria nao mapeia aresta {j} em aresta");
    }
    s
}

/// Conjugacao s^-1 * c * s so nas arestas.
pub fn edge_conj(c: &EdgeCube, s: &EdgeCube, s_inv: &EdgeCube) -> EdgeCube {
    edge_mult(&edge_mult(s_inv, c), s)
}

pub fn edges_of_raw(raw: usize) -> EdgeCube {
    let mut e = EDGE_ID;
    set_slice((raw / N_FLIP) as u16, &mut e.ep);
    set_flip((raw % N_FLIP) as u16, &mut e.eo);
    e
}

pub fn raw_of_edges(e: &EdgeCube) -> usize {
    get_slice(&e.ep) as usize * N_FLIP + get_flip(&e.eo) as usize
}

// ---------------------------------------------------------------------------
// Tabelas de simetria + tabela grande
// ---------------------------------------------------------------------------

pub struct BigP1 {
    /// twist_conj[twist * 16 + s] = coordenada twist conjugada pela simetria s.
    pub twist_conj: Vec<u16>,
    /// raw_to_sym[raw] = (classe << 4) | s, onde conjugar raw por s da o representante.
    pub raw_to_sym: Vec<u32>,
    /// class_rep[classe] = coordenada raw do representante.
    pub class_rep: Vec<u32>,
    /// stab[classe] = mascara das simetrias que fixam o representante.
    pub stab: Vec<u16>,
    /// Distancia exata da fase 1: dist[classe * 2187 + twist].
    pub dist: Vec<u8>,
}

impl BigP1 {
    /// Distancia exata da fase 1 para o estado (twist, flip, slice).
    #[inline(always)]
    pub fn h(&self, twist: u16, flip: u16, slice: u16) -> u8 {
        let raw = slice as usize * N_FLIP + flip as usize;
        let cs = self.raw_to_sym[raw] as usize;
        let t = self.twist_conj[(twist as usize) * N_SYM + (cs & 15)] as usize;
        self.dist[(cs >> 4) * N_TWIST + t]
    }

    /// Constroi tudo, usando (ou criando) o cache de distancias em `cache`.
    pub fn load_or_build(t: &Tables, cache: Option<&std::path::Path>, verbose: bool) -> BigP1 {
        let mut big = Self::build_sym_tables();
        if let Some(dist) = cache.and_then(|p| read_cache(p, b"CUB1", N_CLASS * N_TWIST)) {
            big.dist = dist;
            return big;
        }
        let fus_move = big.build_fus_move(t);
        big.dist = build_dist(t, &big.twist_conj, &fus_move, &big.stab, verbose);
        if let Some(path) = cache {
            if let Err(e) = write_cache(path, b"CUB1", &big.dist) {
                eprintln!("aviso: nao consegui salvar o cache em {}: {e}", path.display());
            }
        }
        big
    }

    /// Tabelas de simetria (deterministicas, ~1 s em paralelo); dist fica vazio.
    pub fn build_sym_tables() -> BigP1 {
        let syms = symmetries();
        let edge_syms: Vec<EdgeCube> = syms.iter().map(edge_cube_of).collect();
        let edge_invs: Vec<EdgeCube> = edge_syms.iter().map(edge_inverse).collect();

        let twist_conj = build_twist_conj(&syms);

        // Menor conjugado de cada raw e por qual simetria (paralelo por blocos).
        let mut min_of = vec![(0u32, 0u8); N_RAW];
        {
            let next = AtomicUsize::new(0);
            let chunk = 8192;
            let slots = std::sync::Mutex::new(&mut min_of);
            std::thread::scope(|sc| {
                let handles: Vec<_> = (0..workers())
                    .map(|_| {
                        let next = &next;
                        let edge_syms = &edge_syms;
                        let edge_invs = &edge_invs;
                        sc.spawn(move || {
                            let mut local: Vec<(usize, Vec<(u32, u8)>)> = Vec::new();
                            loop {
                                let start = next.fetch_add(chunk, Ordering::Relaxed);
                                if start >= N_RAW {
                                    break;
                                }
                                let end = (start + chunk).min(N_RAW);
                                let mut buf = Vec::with_capacity(end - start);
                                for raw in start..end {
                                    let c = edges_of_raw(raw);
                                    let mut best = u32::MAX;
                                    let mut bs = 0u8;
                                    for s in 0..N_SYM {
                                        let r2 = raw_of_edges(&edge_conj(
                                            &c,
                                            &edge_syms[s],
                                            &edge_invs[s],
                                        )) as u32;
                                        if r2 < best {
                                            best = r2;
                                            bs = s as u8;
                                        }
                                    }
                                    buf.push((best, bs));
                                }
                                local.push((start, buf));
                            }
                            local
                        })
                    })
                    .collect();
                let mut guard = slots.lock().unwrap();
                for h in handles {
                    for (start, buf) in h.join().unwrap() {
                        for (i, v) in buf.into_iter().enumerate() {
                            guard[start + i] = v;
                        }
                    }
                }
            });
        }

        // Numeracao deterministica das classes, em ordem crescente de raw.
        let mut raw_to_sym = vec![0u32; N_RAW];
        let mut class_rep: Vec<u32> = Vec::with_capacity(N_CLASS);
        let mut rep_class = vec![u32::MAX; N_RAW];
        for raw in 0..N_RAW {
            let rep = min_of[raw].0 as usize;
            if rep == raw && rep_class[rep] == u32::MAX {
                rep_class[rep] = class_rep.len() as u32;
                class_rep.push(rep as u32);
            }
        }
        for raw in 0..N_RAW {
            let (rep, s) = min_of[raw];
            raw_to_sym[raw] = (rep_class[rep as usize] << 4) | s as u32;
        }
        assert_eq!(
            class_rep.len(),
            N_CLASS,
            "contagem de classes de simetria inesperada - simetrias erradas?"
        );

        // Estabilizador de cada representante.
        let mut stab = vec![0u16; N_CLASS];
        for (ci, &rep) in class_rep.iter().enumerate() {
            let c = edges_of_raw(rep as usize);
            let mut mask = 0u16;
            for s in 0..N_SYM {
                if raw_of_edges(&edge_conj(&c, &edge_syms[s], &edge_invs[s])) == rep as usize {
                    mask |= 1 << s;
                }
            }
            stab[ci] = mask;
        }

        BigP1 { twist_conj, raw_to_sym, class_rep, stab, dist: Vec::new() }
    }

    /// Movimento sobre classes: fus_move[classe * 18 + m] = (classe' << 4) | s'.
    fn build_fus_move(&self, t: &Tables) -> Vec<u32> {
        let mut fus_move = vec![0u32; N_CLASS * 18];
        for (ci, &rep) in self.class_rep.iter().enumerate() {
            let slice = rep as usize / N_FLIP;
            let flip = rep as usize % N_FLIP;
            for m in 0..18 {
                let s2 = t.slice_move[slice * 18 + m] as usize;
                let f2 = t.flip_move[flip * 18 + m] as usize;
                fus_move[ci * 18 + m] = self.raw_to_sym[s2 * N_FLIP + f2];
            }
        }
        fus_move
    }
}

fn build_twist_conj(syms: &[SymXform]) -> Vec<u16> {
    let mut twist_conj = vec![0u16; N_TWIST * N_SYM];
    let next = AtomicUsize::new(0);
    let out = std::sync::Mutex::new(&mut twist_conj);
    std::thread::scope(|sc| {
        let handles: Vec<_> = (0..workers())
            .map(|_| {
                let next = &next;
                sc.spawn(move || {
                    let mut local: Vec<(usize, [u16; N_SYM])> = Vec::new();
                    loop {
                        let tw = next.fetch_add(1, Ordering::Relaxed);
                        if tw >= N_TWIST {
                            break;
                        }
                        let mut c = SOLVED;
                        set_twist(tw as u16, &mut c.co);
                        let f = to_facelets(&c);
                        let mut row = [0u16; N_SYM];
                        for (s, x) in syms.iter().enumerate() {
                            let c2 = to_cubie(&apply_xform(x, &f))
                                .expect("conjugado de estado valido e valido");
                            debug_assert_eq!(c2.cp, SOLVED.cp);
                            row[s] = get_twist(&c2.co);
                        }
                        local.push((tw, row));
                    }
                    local
                })
            })
            .collect();
        let mut guard = out.lock().unwrap();
        for h in handles {
            for (tw, row) in h.join().unwrap() {
                for s in 0..N_SYM {
                    guard[tw * N_SYM + s] = row[s];
                }
            }
        }
    });
    twist_conj
}

// Vec<u8> <-> Vec<AtomicU8>: AtomicU8 tem a mesma representacao em memoria de u8.
fn as_atomic(v: Vec<u8>) -> Vec<AtomicU8> {
    let mut v = std::mem::ManuallyDrop::new(v);
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut AtomicU8, v.len(), v.capacity()) }
}

fn as_plain(v: Vec<AtomicU8>) -> Vec<u8> {
    let mut v = std::mem::ManuallyDrop::new(v);
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

fn build_dist(
    t: &Tables,
    twist_conj: &[u16],
    fus_move: &[u32],
    stab: &[u16],
    verbose: bool,
) -> Vec<u8> {
    let n = N_CLASS * N_TWIST;
    let dist = as_atomic(vec![255u8; n]);
    // Estado resolvido: raw 0 e o menor raw possivel, logo classe 0; twist 0.
    dist[0].store(0, Ordering::Relaxed);

    let mut depth = 0u8;
    loop {
        let found = AtomicUsize::new(0);
        let next = AtomicUsize::new(0);
        let chunk = 64 * N_TWIST; // blocos de 64 classes
        std::thread::scope(|sc| {
            for _ in 0..workers() {
                let dist = &dist;
                let found = &found;
                let next = &next;
                sc.spawn(move || {
                    let mut local = 0usize;
                    loop {
                        let start = next.fetch_add(chunk, Ordering::Relaxed);
                        if start >= n {
                            break;
                        }
                        let end = (start + chunk).min(n);
                        for idx in start..end {
                            if dist[idx].load(Ordering::Relaxed) != depth {
                                continue;
                            }
                            let class = idx / N_TWIST;
                            let twist = idx % N_TWIST;
                            for m in 0..18 {
                                let cs = fus_move[class * 18 + m] as usize;
                                let c2 = cs >> 4;
                                let s2 = cs & 15;
                                let tm = t.twist_move[twist * 18 + m] as usize;
                                let t2 = twist_conj[tm * N_SYM + s2] as usize;
                                let j = c2 * N_TWIST + t2;
                                if dist[j].load(Ordering::Relaxed) == 255 {
                                    dist[j].store(depth + 1, Ordering::Relaxed);
                                    local += 1;
                                    // Estados equivalentes pelo estabilizador do
                                    // representante recebem a mesma distancia.
                                    let mask = stab[c2];
                                    if mask != 1 {
                                        for s in 1..N_SYM {
                                            if mask & (1 << s) != 0 {
                                                let ts =
                                                    twist_conj[t2 * N_SYM + s] as usize;
                                                let js = c2 * N_TWIST + ts;
                                                if dist[js].load(Ordering::Relaxed) == 255 {
                                                    dist[js].store(depth + 1, Ordering::Relaxed);
                                                    local += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    found.fetch_add(local, Ordering::Relaxed);
                });
            }
        });
        let new_states = found.load(Ordering::Relaxed);
        if verbose {
            println!("    profundidade {:2}: {new_states} estados novos", depth + 1);
        }
        if new_states == 0 {
            break;
        }
        depth += 1;
        assert!(depth <= 14, "BFS da fase 1 passou da profundidade esperada");
    }
    as_plain(dist)
}

// ---------------------------------------------------------------------------
// Cache em disco (so a tabela de distancias; o resto reconstroi em ~1 s)
// ---------------------------------------------------------------------------

fn read_cache(path: &std::path::Path, magic: &[u8; 4], n: usize) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    if data.len() != 8 + n || &data[0..4] != magic {
        return None;
    }
    let ver = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if ver != CACHE_VERSION {
        return None;
    }
    let mut dist = data;
    dist.drain(0..8);
    Some(dist)
}

fn write_cache(path: &std::path::Path, magic: &[u8; 4], dist: &[u8]) -> std::io::Result<()> {
    let mut data = Vec::with_capacity(8 + dist.len());
    data.extend_from_slice(magic);
    data.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    data.extend_from_slice(dist);
    std::fs::write(path, data)
}

// ---------------------------------------------------------------------------
// Tabela grande da fase 2: permutacao dos cantos (2768 classes) x arestas U/D
//
// Distancia minima, com os 10 movimentos de G1, para resolver cantos + arestas
// U/D *ignorando* a permutacao da fatia — limite inferior forte (quase exato)
// da fase 2. So permutacoes: a conjugacao e uma troca de indices, sem nenhuma
// aritmetica de orientacao (o espelho nao incomoda).
// ---------------------------------------------------------------------------

/// Acao da simetria sobre os 8 encaixes de canto (ignora orientacao):
/// cp[k] = j significa que o conteudo do encaixe j vai para o encaixe k.
pub fn corner_perm_of(x: &SymXform) -> [u8; 8] {
    let mut cp = [0u8; 8];
    for j in 0..8 {
        let mut set: [usize; 3] = [0; 3];
        for (i, &p) in CORNER_FACELET[j].iter().enumerate() {
            set[i] = x.perm[p];
        }
        set.sort_unstable();
        let mut found = false;
        for k in 0..8 {
            let mut tgt = CORNER_FACELET[k];
            tgt.sort_unstable();
            if tgt == set {
                cp[k] = j as u8;
                found = true;
                break;
            }
        }
        debug_assert!(found, "simetria nao mapeia canto {j} em canto");
    }
    cp
}

fn perm8_mult(a: &[u8; 8], b: &[u8; 8]) -> [u8; 8] {
    let mut r = [0u8; 8];
    for i in 0..8 {
        r[i] = a[b[i] as usize];
    }
    r
}

pub fn perm8_inverse(a: &[u8; 8]) -> [u8; 8] {
    let mut r = [0u8; 8];
    for i in 0..8 {
        r[a[i] as usize] = i as u8;
    }
    r
}

/// Conjugacao s^-1 * c * s de uma permutacao de cantos.
pub fn cperm_conj(cp: &[u8; 8], s: &[u8; 8], s_inv: &[u8; 8]) -> [u8; 8] {
    perm8_mult(&perm8_mult(s_inv, cp), s)
}

pub struct BigP2 {
    /// cperm_rts[cperm] = (classe << 4) | s, onde conjugar por s da o representante.
    pub cperm_rts: Vec<u32>,
    /// class_rep[classe] = coordenada cperm do representante.
    pub class_rep: Vec<u16>,
    /// stab[classe] = simetrias que fixam o representante.
    pub stab: Vec<u16>,
    /// uperm_conj[uperm * 16 + s] = coordenada uperm conjugada por s.
    pub uperm_conj: Vec<u16>,
    /// dist[classe * 40320 + uperm], distancia com os 10 movimentos de G1.
    pub dist: Vec<u8>,
}

impl BigP2 {
    /// Limite inferior da fase 2 para (cperm, uperm), ignorando a fatia.
    #[inline(always)]
    pub fn h2(&self, cperm: u16, uperm: u16) -> u8 {
        let cs = self.cperm_rts[cperm as usize] as usize;
        let u = self.uperm_conj[(uperm as usize) * N_SYM + (cs & 15)] as usize;
        self.dist[(cs >> 4) * N_UPERM + u]
    }

    pub fn load_or_build(t: &Tables, cache: Option<&std::path::Path>, verbose: bool) -> BigP2 {
        let mut big = Self::build_sym_tables();
        if let Some(dist) = cache.and_then(|p| read_cache(p, b"CUB2", N_CLASS2 * N_UPERM)) {
            big.dist = dist;
            return big;
        }
        big.dist = build_dist2(t, &big, verbose);
        if let Some(path) = cache {
            if let Err(e) = write_cache(path, b"CUB2", &big.dist) {
                eprintln!("aviso: nao consegui salvar o cache em {}: {e}", path.display());
            }
        }
        big
    }

    pub fn build_sym_tables() -> BigP2 {
        let syms = symmetries();
        let corner_syms: Vec<[u8; 8]> = syms.iter().map(corner_perm_of).collect();
        let corner_invs: Vec<[u8; 8]> = corner_syms.iter().map(perm8_inverse).collect();
        let edge_syms: Vec<EdgeCube> = syms.iter().map(edge_cube_of).collect();
        let edge_invs: Vec<EdgeCube> = edge_syms.iter().map(edge_inverse).collect();

        // Classes das permutacoes de canto.
        let mut cperm_rts = vec![0u32; N_CPERM];
        let mut class_rep: Vec<u16> = Vec::with_capacity(N_CLASS2);
        let mut rep_class = vec![u32::MAX; N_CPERM];
        let mut min_of = vec![(0u16, 0u8); N_CPERM];
        let mut cp = [0u8; 8];
        for i in 0..N_CPERM {
            perm_from_index(i as u32, 8, &mut cp);
            let mut best = u16::MAX;
            let mut bs = 0u8;
            for s in 0..N_SYM {
                let c2 = cperm_conj(&cp, &corner_syms[s], &corner_invs[s]);
                let r2 = perm_index(&c2) as u16;
                if r2 < best {
                    best = r2;
                    bs = s as u8;
                }
            }
            min_of[i] = (best, bs);
        }
        for i in 0..N_CPERM {
            let rep = min_of[i].0 as usize;
            if rep == i && rep_class[rep] == u32::MAX {
                rep_class[rep] = class_rep.len() as u32;
                class_rep.push(rep as u16);
            }
        }
        for i in 0..N_CPERM {
            let (rep, s) = min_of[i];
            cperm_rts[i] = (rep_class[rep as usize] << 4) | s as u32;
        }
        assert_eq!(
            class_rep.len(),
            N_CLASS2,
            "contagem de classes de cperm inesperada - simetrias erradas?"
        );

        let mut stab = vec![0u16; N_CLASS2];
        for (ci, &rep) in class_rep.iter().enumerate() {
            perm_from_index(rep as u32, 8, &mut cp);
            let mut mask = 0u16;
            for s in 0..N_SYM {
                let c2 = cperm_conj(&cp, &corner_syms[s], &corner_invs[s]);
                if perm_index(&c2) as u16 == rep {
                    mask |= 1 << s;
                }
            }
            stab[ci] = mask;
        }

        // Conjugacao da permutacao das arestas U/D (a fatia fica parada).
        let mut uperm_conj = vec![0u16; N_UPERM * N_SYM];
        let mut p8 = [0u8; 8];
        for u in 0..N_UPERM {
            perm_from_index(u as u32, 8, &mut p8);
            let mut e = EDGE_ID;
            e.ep[0..8].copy_from_slice(&p8);
            for s in 0..N_SYM {
                let c2 = edge_conj(&e, &edge_syms[s], &edge_invs[s]);
                let mut q = [0u8; 8];
                q.copy_from_slice(&c2.ep[0..8]);
                uperm_conj[u * N_SYM + s] = perm_index(&q) as u16;
            }
        }

        BigP2 { cperm_rts, class_rep, stab, uperm_conj, dist: Vec::new() }
    }
}

fn build_dist2(t: &Tables, big: &BigP2, verbose: bool) -> Vec<u8> {
    // Movimento sobre classes: (classe' << 4) | s' para cada um dos 10 movimentos.
    let mut cmove = vec![0u32; N_CLASS2 * N_P2_MOVES];
    for (ci, &rep) in big.class_rep.iter().enumerate() {
        for j in 0..N_P2_MOVES {
            let c2 = t.cperm_move[rep as usize * N_P2_MOVES + j] as usize;
            cmove[ci * N_P2_MOVES + j] = big.cperm_rts[c2];
        }
    }

    let n = N_CLASS2 * N_UPERM;
    let dist = as_atomic(vec![255u8; n]);
    dist[0].store(0, Ordering::Relaxed);

    let mut depth = 0u8;
    loop {
        let found = AtomicUsize::new(0);
        let next = AtomicUsize::new(0);
        let chunk = 8 * N_UPERM; // blocos de 8 classes
        std::thread::scope(|sc| {
            for _ in 0..workers() {
                let dist = &dist;
                let found = &found;
                let next = &next;
                let cmove = &cmove;
                sc.spawn(move || {
                    let mut local = 0usize;
                    loop {
                        let start = next.fetch_add(chunk, Ordering::Relaxed);
                        if start >= n {
                            break;
                        }
                        let end = (start + chunk).min(n);
                        for idx in start..end {
                            if dist[idx].load(Ordering::Relaxed) != depth {
                                continue;
                            }
                            let class = idx / N_UPERM;
                            let uperm = idx % N_UPERM;
                            for j in 0..N_P2_MOVES {
                                let cs = cmove[class * N_P2_MOVES + j] as usize;
                                let c2 = cs >> 4;
                                let s2 = cs & 15;
                                let um = t.uperm_move[uperm * N_P2_MOVES + j] as usize;
                                let u2 = big.uperm_conj[um * N_SYM + s2] as usize;
                                let k = c2 * N_UPERM + u2;
                                if dist[k].load(Ordering::Relaxed) == 255 {
                                    dist[k].store(depth + 1, Ordering::Relaxed);
                                    local += 1;
                                    let mask = big.stab[c2];
                                    if mask != 1 {
                                        for s in 1..N_SYM {
                                            if mask & (1 << s) != 0 {
                                                let us =
                                                    big.uperm_conj[u2 * N_SYM + s] as usize;
                                                let ks = c2 * N_UPERM + us;
                                                if dist[ks].load(Ordering::Relaxed) == 255 {
                                                    dist[ks].store(depth + 1, Ordering::Relaxed);
                                                    local += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    found.fetch_add(local, Ordering::Relaxed);
                });
            }
        });
        let new_states = found.load(Ordering::Relaxed);
        if verbose {
            println!("    fase 2, profundidade {:2}: {new_states} estados novos", depth + 1);
        }
        if new_states == 0 {
            break;
        }
        depth += 1;
        assert!(depth <= 20, "BFS da fase 2 passou da profundidade esperada");
    }
    as_plain(dist)
}


