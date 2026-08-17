//! Tabela "X": a heuristica grande do solver otimo.
//!
//! Espaco: orientacao dos cantos (2187) x orientacao das arestas (2048) x
//! posicao ORDENADA das 4 arestas da fatia (11880) — a fase 1 refinada com a
//! identidade de cada aresta da fatia. Sao 2187 x 2048 x 11880 = 53 bilhoes de
//! estados; as 16 simetrias do eixo U/D reduzem (flip x epos) de 24.330.240
//! para ~1,52 milhao de classes, e a tabela final tem ~3,3 bilhoes de entradas.
//!
//! Em 1 byte seriam 3,3 GB; guardamos a DISTANCIA MOD 3 em 2 bits (~830 MB).
//! O mod 3 basta porque cada movimento muda a distancia em -1, 0 ou +1: a
//! diferenca (mod 3) entre vizinhos identifica o delta exato, entao a busca
//! carrega a distancia exata incrementalmente. A distancia da raiz e computada
//! "descendo": de qualquer estado sempre existe um vizinho com delta -1.
//!
//! E um limite inferior exato num quociente mais fino que a fase 1 — vale para
//! o espaco completo dos 18 movimentos, que e o que o solver otimo precisa.

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};

use crate::coord::*;
use crate::sym::{
    self, as_plain, edge_conj, edge_cube_of, edge_inverse, symmetries, workers, EdgeCube, EDGE_ID,
    N_SYM,
};
use crate::tables::Tables;

pub const N_RAW2: usize = N_EPOS * N_FLIP; // 24.330.240
const CACHE_MAGIC: &[u8; 4] = b"CUBX";
const CACHE_VERSION: u32 = 1;

pub struct BigX {
    pub n_class: usize,
    /// raw2_to_sym[epos * 2048 + flip] = (classe << 4) | s.
    pub raw2_to_sym: Vec<u32>,
    /// twist_conj[twist * 16 + s] (identica a da fase 1).
    pub twist_conj: Vec<u16>,
    /// epos_move[epos * 18 + m].
    pub epos_move: Vec<u16>,
    /// dist mod 3, 2 bits por entrada: [classe * 2187 + twist].
    pub dist: Vec<u8>,
    pub epos_solved: u16,
}

#[inline(always)]
fn get2(dist: &[u8], i: usize) -> u8 {
    (dist[i >> 2] >> ((i & 3) << 1)) & 3
}

impl BigX {
    #[inline(always)]
    fn index(&self, twist: u16, flip: u16, epos: u16) -> usize {
        let raw2 = epos as usize * N_FLIP + flip as usize;
        let cs = self.raw2_to_sym[raw2] as usize;
        let t = self.twist_conj[(twist as usize) * N_SYM + (cs & 15)] as usize;
        (cs >> 4) * N_TWIST + t
    }

    /// Distancia mod 3 do estado.
    #[inline(always)]
    pub fn m3(&self, twist: u16, flip: u16, epos: u16) -> u8 {
        get2(&self.dist, self.index(twist, flip, epos))
    }

    #[inline(always)]
    pub fn is_goal(&self, twist: u16, flip: u16, epos: u16) -> bool {
        twist == 0 && flip == 0 && epos == self.epos_solved
    }

    /// Distancia exata, "descendo" pela tabela (so na raiz da busca).
    pub fn exact(&self, t: &Tables, mut twist: u16, mut flip: u16, mut epos: u16) -> u8 {
        let mut steps = 0u8;
        let mut cur = self.m3(twist, flip, epos);
        while !self.is_goal(twist, flip, epos) {
            let want = (cur + 2) % 3; // vizinho com distancia -1
            let mut advanced = false;
            for m in 0..18 {
                let t2 = t.twist_move[twist as usize * 18 + m];
                let f2 = t.flip_move[flip as usize * 18 + m];
                let e2 = self.epos_move[epos as usize * 18 + m];
                if self.m3(t2, f2, e2) == want {
                    twist = t2;
                    flip = f2;
                    epos = e2;
                    cur = want;
                    steps += 1;
                    advanced = true;
                    break;
                }
            }
            assert!(advanced, "tabela X sem vizinho descendente - corrompida?");
            assert!(steps < 40, "descida na tabela X nao terminou");
        }
        steps
    }

    pub fn load_or_build(t: &Tables, cache: Option<&std::path::Path>, verbose: bool) -> BigX {
        let epos_move = build_epos_move(t);
        let syms = symmetries();
        let twist_conj = sym::build_twist_conj(&syms);
        let epos_solved = epos_solved();

        if let Some(path) = cache {
            if let Some((raw2_to_sym, dist, n_class)) = read_cache(path) {
                return BigX { n_class, raw2_to_sym, twist_conj, epos_move, dist, epos_solved };
            }
        }

        // --- classes de (flip x epos) sob as 16 simetrias ------------------
        let edge_syms: Vec<EdgeCube> = syms.iter().map(edge_cube_of).collect();
        let edge_invs: Vec<EdgeCube> = edge_syms.iter().map(edge_inverse).collect();

        let mut min_of = vec![(0u32, 0u8); N_RAW2];
        {
            let next = AtomicUsize::new(0);
            let chunk = 16384;
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
                                if start >= N_RAW2 {
                                    break;
                                }
                                let end = (start + chunk).min(N_RAW2);
                                let mut buf = Vec::with_capacity(end - start);
                                for raw2 in start..end {
                                    let c = edges_of_raw2(raw2);
                                    let mut best = u32::MAX;
                                    let mut bs = 0u8;
                                    for s in 0..N_SYM {
                                        let r2 = raw2_of_edges(&edge_conj(
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

        let mut raw2_to_sym = vec![0u32; N_RAW2];
        let mut class_rep: Vec<u32> = Vec::new();
        let mut rep_class = vec![u32::MAX; N_RAW2];
        for raw2 in 0..N_RAW2 {
            let rep = min_of[raw2].0 as usize;
            if rep == raw2 && rep_class[rep] == u32::MAX {
                rep_class[rep] = class_rep.len() as u32;
                class_rep.push(rep as u32);
            }
        }
        for raw2 in 0..N_RAW2 {
            let (rep, s) = min_of[raw2];
            raw2_to_sym[raw2] = (rep_class[rep as usize] << 4) | s as u32;
        }
        drop(min_of);
        let n_class = class_rep.len();
        // Sanidade: no minimo N_RAW2/16 (todas as orbitas cheias); acima disso
        // so o excesso dos estabilizadores nao-triviais.
        assert!(
            n_class >= N_RAW2 / 16 && n_class < N_RAW2 / 16 + 60_000,
            "contagem de classes {n_class} fora do esperado - simetrias erradas?"
        );
        if verbose {
            println!("    {n_class} classes de (flip x epos)");
        }

        let mut stab = vec![0u16; n_class];
        for (ci, &rep) in class_rep.iter().enumerate() {
            let c = edges_of_raw2(rep as usize);
            let mut mask = 0u16;
            for s in 0..N_SYM {
                if raw2_of_edges(&edge_conj(&c, &edge_syms[s], &edge_invs[s])) == rep as usize {
                    mask |= 1 << s;
                }
            }
            stab[ci] = mask;
        }

        // Movimentos sobre classes.
        let mut cls_move = vec![0u32; n_class * 18];
        {
            let next = AtomicUsize::new(0);
            let chunk = 4096;
            let slots = std::sync::Mutex::new(&mut cls_move);
            std::thread::scope(|sc| {
                let handles: Vec<_> = (0..workers())
                    .map(|_| {
                        let next = &next;
                        let class_rep = &class_rep;
                        let raw2_to_sym = &raw2_to_sym;
                        let epos_move = &epos_move;
                        sc.spawn(move || {
                            let mut local: Vec<(usize, Vec<u32>)> = Vec::new();
                            loop {
                                let start = next.fetch_add(chunk, Ordering::Relaxed);
                                if start >= class_rep.len() {
                                    break;
                                }
                                let end = (start + chunk).min(class_rep.len());
                                let mut buf = Vec::with_capacity((end - start) * 18);
                                for ci in start..end {
                                    let rep = class_rep[ci] as usize;
                                    let epos = rep / N_FLIP;
                                    let flip = rep % N_FLIP;
                                    for m in 0..18 {
                                        let e2 = epos_move[epos * 18 + m] as usize;
                                        let f2 = t.flip_move[flip * 18 + m] as usize;
                                        buf.push(raw2_to_sym[e2 * N_FLIP + f2]);
                                    }
                                }
                                local.push((start * 18, buf));
                            }
                            local
                        })
                    })
                    .collect();
                let mut guard = slots.lock().unwrap();
                for h in handles {
                    for (off, buf) in h.join().unwrap() {
                        guard[off..off + buf.len()].copy_from_slice(&buf);
                    }
                }
            });
        }

        // --- BFS com fronteira em bitmap e distancias mod 3 ----------------
        let dist = build_dist_x(t, &twist_conj, &cls_move, &stab, &raw2_to_sym, n_class, verbose);

        let big = BigX { n_class, raw2_to_sym, twist_conj, epos_move, dist, epos_solved };
        if let Some(path) = cache {
            if let Err(e) = write_cache(path, &big) {
                eprintln!("aviso: nao consegui salvar o cache em {}: {e}", path.display());
            }
        }
        big
    }
}

fn edges_of_raw2(raw2: usize) -> EdgeCube {
    let mut e = EDGE_ID;
    set_epos((raw2 / N_FLIP) as u16, &mut e.ep);
    set_flip((raw2 % N_FLIP) as u16, &mut e.eo);
    e
}

fn raw2_of_edges(e: &EdgeCube) -> usize {
    get_epos(&e.ep) as usize * N_FLIP + get_flip(&e.eo) as usize
}

fn build_epos_move(t: &Tables) -> Vec<u16> {
    let mut out = vec![0u16; N_EPOS * 18];
    let mut ep = [0u8; 12];
    for e in 0..N_EPOS {
        set_epos(e as u16, &mut ep);
        for m in 0..18 {
            // so a permutacao importa: aplica a parte de arestas do movimento
            let mv = &t.mc[m].ep;
            let mut ep2 = [0u8; 12];
            for i in 0..12 {
                ep2[i] = ep[mv[i] as usize];
            }
            out[e * 18 + m] = get_epos(&ep2);
        }
    }
    out
}

fn build_dist_x(
    t: &Tables,
    twist_conj: &[u16],
    cls_move: &[u32],
    stab: &[u16],
    raw2_to_sym: &[u32],
    n_class: usize,
    verbose: bool,
) -> Vec<u8> {
    let n = n_class * N_TWIST;
    let n_bytes = (n + 3) / 4;
    let dist: Vec<AtomicU8> = sym::as_atomic(vec![0xFFu8; n_bytes]); // tudo "3" = nao visitado
    let n_words = (n + 63) / 64;
    let mut cur: Vec<AtomicU64> = (0..n_words).map(|_| AtomicU64::new(0)).collect();
    let mut nxt: Vec<AtomicU64> = (0..n_words).map(|_| AtomicU64::new(0)).collect();

    // semente: estado resolvido
    let raw2_id = epos_solved() as usize * N_FLIP + 0;
    let seed_class = (raw2_to_sym[raw2_id] >> 4) as usize;
    let seed_sym = (raw2_to_sym[raw2_id] & 15) as usize;
    let seed_twist = twist_conj[seed_sym] as usize; // twist 0 conjugado
    let seed = seed_class * N_TWIST + seed_twist;
    set2(&dist, seed, 0);
    cur[seed / 64].store(1u64 << (seed % 64), Ordering::Relaxed);

    let mut depth = 0u8;
    let mut total = 1usize;
    loop {
        let found = AtomicUsize::new(0);
        let next_word = AtomicUsize::new(0);
        let chunk = 1 << 14;
        let v = ((depth + 1) % 3) as u8;
        std::thread::scope(|sc| {
            for _ in 0..workers() {
                let dist = &dist;
                let cur = &cur;
                let nxt = &nxt;
                let found = &found;
                let next_word = &next_word;
                sc.spawn(move || {
                    let mut local = 0usize;
                    loop {
                        let start = next_word.fetch_add(chunk, Ordering::Relaxed);
                        if start >= n_words {
                            break;
                        }
                        let end = (start + chunk).min(n_words);
                        for w in start..end {
                            let mut bits = cur[w].load(Ordering::Relaxed);
                            if bits == 0 {
                                continue;
                            }
                            while bits != 0 {
                                let b = bits.trailing_zeros() as usize;
                                bits &= bits - 1;
                                let idx = w * 64 + b;
                                let class = idx / N_TWIST;
                                let twist = idx % N_TWIST;
                                for m in 0..18 {
                                    let cs = cls_move[class * 18 + m] as usize;
                                    let c2 = cs >> 4;
                                    let s2 = cs & 15;
                                    let tm = t.twist_move[twist * 18 + m] as usize;
                                    let t2 = twist_conj[tm * N_SYM + s2] as usize;
                                    let j = c2 * N_TWIST + t2;
                                    if try_set2(dist, j, v) {
                                        nxt[j / 64].fetch_or(1u64 << (j % 64), Ordering::Relaxed);
                                        local += 1;
                                        let mask = stab[c2];
                                        if mask != 1 {
                                            for s in 1..N_SYM {
                                                if mask & (1 << s) != 0 {
                                                    let ts = twist_conj[t2 * N_SYM + s] as usize;
                                                    let js = c2 * N_TWIST + ts;
                                                    if try_set2(dist, js, v) {
                                                        nxt[js / 64].fetch_or(
                                                            1u64 << (js % 64),
                                                            Ordering::Relaxed,
                                                        );
                                                        local += 1;
                                                    }
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
        total += new_states;
        if verbose {
            println!(
                "    profundidade {:2}: {new_states} estados novos ({:.1}% do espaco)",
                depth + 1,
                total as f64 / n as f64 * 100.0
            );
        }
        if new_states == 0 {
            break;
        }
        std::mem::swap(&mut cur, &mut nxt);
        // zera a proxima fronteira em paralelo (416 MB)
        let next_word2 = AtomicUsize::new(0);
        std::thread::scope(|sc| {
            for _ in 0..workers() {
                let nxt = &nxt;
                let next_word2 = &next_word2;
                sc.spawn(move || loop {
                    let start = next_word2.fetch_add(1 << 16, Ordering::Relaxed);
                    if start >= n_words {
                        break;
                    }
                    let end = (start + (1 << 16)).min(n_words);
                    for w in start..end {
                        nxt[w].store(0, Ordering::Relaxed);
                    }
                });
            }
        });
        depth += 1;
        assert!(depth <= 20, "BFS da tabela X passou da profundidade esperada");
    }
    assert_eq!(total, n, "tabela X incompleta: {total} de {n} estados alcancados");
    as_plain(dist)
}

#[inline]
fn set2(dist: &[AtomicU8], i: usize, v: u8) {
    let byte = i >> 2;
    let shift = (i & 3) << 1;
    let mut old = dist[byte].load(Ordering::Relaxed);
    loop {
        let new = (old & !(3 << shift)) | (v << shift);
        match dist[byte].compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(x) => old = x,
        }
    }
}

/// Marca o campo com `v` se ainda estiver em 3 (nao visitado). true = fomos nos.
#[inline]
fn try_set2(dist: &[AtomicU8], i: usize, v: u8) -> bool {
    let byte = i >> 2;
    let shift = (i & 3) << 1;
    let mut old = dist[byte].load(Ordering::Relaxed);
    loop {
        if (old >> shift) & 3 != 3 {
            return false;
        }
        let new = (old & !(3 << shift)) | (v << shift);
        match dist[byte].compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(x) => old = x,
        }
    }
}

// ---------------------------------------------------------------------------
// Cache: raw2_to_sym + dist num arquivo so (~930 MB)
// ---------------------------------------------------------------------------

fn read_cache(path: &std::path::Path) -> Option<(Vec<u32>, Vec<u8>, usize)> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 16 || &data[0..4] != CACHE_MAGIC {
        return None;
    }
    let ver = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if ver != CACHE_VERSION {
        return None;
    }
    let n_class = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let rts_bytes = N_RAW2 * 4;
    let dist_bytes = (n_class * N_TWIST + 3) / 4;
    if data.len() != 16 + rts_bytes + dist_bytes {
        return None;
    }
    let mut raw2_to_sym = vec![0u32; N_RAW2];
    for (i, v) in raw2_to_sym.iter_mut().enumerate() {
        let o = 16 + i * 4;
        *v = u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    }
    let dist = data[16 + rts_bytes..].to_vec();
    Some((raw2_to_sym, dist, n_class))
}

fn write_cache(path: &std::path::Path, big: &BigX) -> std::io::Result<()> {
    use std::io::Write;
    let f = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
    w.write_all(CACHE_MAGIC)?;
    w.write_all(&CACHE_VERSION.to_le_bytes())?;
    w.write_all(&(big.n_class as u32).to_le_bytes())?;
    w.write_all(&[0u8; 4])?; // reservado
    for v in &big.raw2_to_sym {
        w.write_all(&v.to_le_bytes())?;
    }
    w.write_all(&big.dist)?;
    w.flush()
}
