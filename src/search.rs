//! Algoritmo de duas fases de Kociemba com busca IDA* e varias threads.
//!
//! Fase 1: leva o cubo para o subgrupo G1 = <U, D, R2, L2, F2, B2>
//!         (orientacoes resolvidas + arestas da fatia do meio na fatia).
//! Fase 2: resolve dentro de G1 usando apenas os 10 movimentos que preservam G1.
//!
//! Cada thread usa uma ordem de faces diferente, entao encontra solucoes de fase 1
//! diferentes primeiro; a melhor solucao global e compartilhada para podar as demais.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::coord::*;
use crate::cube::*;
use crate::tables::Tables;

/// Toda posicao chega a G1 em no maximo 12 movimentos, mas *nao* basta parar ai:
/// uma fase 1 mais longa costuma deixar o cubo numa posicao de G1 muito mais facil,
/// e o total fica menor. Por isso a fase 1 vai ate perto do tamanho maximo aceito.
const MAX_P1_DEPTH: usize = 21;
const MAX_P2_DEPTH: usize = 18;

pub struct Solution {
    pub moves: Vec<u8>,
    pub phase1: usize,
    pub nodes: usize,
    pub p1_sols: usize,
    pub threads: usize,
}

struct Shared {
    best: Mutex<Option<(Vec<u8>, usize)>>,
    best_len: AtomicUsize,
    stop: AtomicBool,
    nodes: AtomicUsize,
    p1_sols: AtomicUsize,
    deadline: Instant,
    hard_deadline: Instant,
    /// Antes deste instante nao aceitamos parar so por ter batido o alvo: sem isso
    /// um cubo a 3 movimentos do fim receberia uma "solucao" de 20 movimentos,
    /// que e a primeira que a busca encontra.
    min_until: Instant,
    target: usize,
}

struct Searcher<'a> {
    t: &'a Tables,
    sh: &'a Shared,
    cube: CubieCube,
    p1: [u8; MAX_P1_DEPTH + 1],
    p2: [u8; MAX_P2_DEPTH + 1],
    p1_len: usize,
    order: [u8; N_MOVES],
    order2: [u8; N_P2_MOVES],
    counter: usize,
    p1_solutions: usize,
    /// Esta thread esta resolvendo o cubo invertido; a sequencia encontrada
    /// precisa ser lida de tras para frente, com cada movimento invertido.
    inverted: bool,
    /// Traducao das faces do referencial girado desta thread para o original.
    face_map: [u8; 6],
}

/// Cada thread percorre as faces em uma rotacao diferente.
fn make_orders(tid: usize) -> ([u8; N_MOVES], [u8; N_P2_MOVES]) {
    let mut order = [0u8; N_MOVES];
    let mut k = 0;
    for i in 0..6 {
        let f = ((i + tid) % 6) as u8;
        for p in 0..3u8 {
            order[k] = f * 3 + p;
            k += 1;
        }
    }
    let mut order2 = [0u8; N_P2_MOVES];
    let mut k = 0;
    for i in 0..6 {
        let f = ((i + tid) % 6) as u8;
        for (j, &m) in P2_MOVES.iter().enumerate() {
            if move_face(m) == f {
                order2[k] = j as u8;
                k += 1;
            }
        }
    }
    (order, order2)
}

impl<'a> Searcher<'a> {
    #[inline]
    fn should_stop(&mut self) -> bool {
        if self.sh.stop.load(Ordering::Relaxed) {
            return true;
        }
        self.counter += 1;
        if self.counter & 0x3FF == 0 {
            let now = Instant::now();
            let best = self.sh.best_len.load(Ordering::Relaxed);
            let has_solution = best != usize::MAX;
            if now >= self.sh.hard_deadline
                || (has_solution && now >= self.sh.deadline)
                || (best <= self.sh.target && now >= self.sh.min_until)
            {
                self.sh.stop.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    fn run(&mut self) {
        let twist = get_twist(&self.cube.co);
        let flip = get_flip(&self.cube.eo);
        let slice = get_slice(&self.cube.ep);
        for d in 0..=MAX_P1_DEPTH {
            if self.should_stop() {
                break;
            }
            // Uma fase 1 com d movimentos so pode melhorar se sobrar espaco para a fase 2.
            if d >= self.sh.best_len.load(Ordering::Relaxed) {
                break;
            }
            self.dfs1(twist, flip, slice, d, 0);
        }
    }

    fn dfs1(&mut self, twist: u16, flip: u16, slice: u16, depth: usize, n: usize) {
        if self.should_stop() {
            return;
        }
        let h = self.t.prun1(twist, flip, slice) as usize;
        if h > depth {
            return;
        }
        if depth == 0 {
            // h == 0  =>  twist == flip == slice == 0  =>  o cubo esta em G1.
            self.p1_len = n;
            self.solve_phase2(n);
            return;
        }
        for i in 0..N_MOVES {
            let m = self.order[i];
            let f = move_face(m);
            if n > 0 {
                let lf = move_face(self.p1[n - 1]);
                // Nao repetir a face; em faces opostas, forcar a ordem U<D, R<L, F<B.
                if f == lf || (lf >= 3 && f + 3 == lf) {
                    continue;
                }
            }
            let mi = m as usize;
            let nt = self.t.twist_move[twist as usize * N_MOVES + mi];
            let nf = self.t.flip_move[flip as usize * N_MOVES + mi];
            let ns = self.t.slice_move[slice as usize * N_MOVES + mi];
            self.p1[n] = m;
            self.dfs1(nt, nf, ns, depth - 1, n + 1);
            if self.sh.stop.load(Ordering::Relaxed) {
                return;
            }
        }
    }

    fn solve_phase2(&mut self, n: usize) {
        self.p1_solutions += 1;
        let bl = self.sh.best_len.load(Ordering::Relaxed);
        if n >= bl {
            return;
        }
        let mut c = self.cube;
        for i in 0..n {
            c = c.multiply(&self.t.mc[self.p1[i] as usize]);
        }
        let cperm = get_cperm(&c.cp);
        let uperm = get_uperm(&c.ep);
        let sperm = get_sperm(&c.ep);

        let max2 = (bl - n - 1).min(MAX_P2_DEPTH);
        let h = self.t.prun2(cperm, uperm, sperm) as usize;
        if h > max2 {
            return;
        }
        for d2 in h..=max2 {
            if self.dfs2(cperm, uperm, sperm, d2, 0) {
                let mut mv = Vec::with_capacity(n + d2);
                mv.extend_from_slice(&self.p1[..n]);
                mv.extend_from_slice(&self.p2[..d2]);
                let mut phase1 = n;
                if self.inverted {
                    mv.reverse();
                    for m in mv.iter_mut() {
                        *m = move_inverse(*m);
                    }
                    phase1 = d2; // no cubo original, a fase 2 invertida vem primeiro
                }
                for m in mv.iter_mut() {
                    *m = self.face_map[(*m / 3) as usize] * 3 + *m % 3;
                }
                let mut g = self.sh.best.lock().unwrap();
                if mv.len() < self.sh.best_len.load(Ordering::Relaxed) {
                    self.sh.best_len.store(mv.len(), Ordering::Relaxed);
                    *g = Some((mv, phase1));
                }
                return;
            }
        }
    }

    fn dfs2(&mut self, cperm: u16, uperm: u16, sperm: u8, depth: usize, n: usize) -> bool {
        if self.should_stop() {
            return false;
        }
        let h = self.t.prun2(cperm, uperm, sperm) as usize;
        if h > depth {
            return false;
        }
        if depth == 0 {
            return true;
        }
        for i in 0..N_P2_MOVES {
            let j = self.order2[i] as usize;
            let m = P2_MOVES[j];
            let f = move_face(m);
            if n > 0 {
                let lf = move_face(self.p2[n - 1]);
                if f == lf || (lf >= 3 && f + 3 == lf) {
                    continue;
                }
            } else if self.p1_len > 0 && f == move_face(self.p1[self.p1_len - 1]) {
                // Na fronteira das fases, repetir a face so geraria uma sequencia
                // equivalente a uma ja explorada (com fase 1 do mesmo tamanho).
                continue;
            }
            let nc = self.t.cperm_move[cperm as usize * N_P2_MOVES + j];
            let nu = self.t.uperm_move[uperm as usize * N_P2_MOVES + j];
            let ns = self.t.sperm_move[sperm as usize * N_P2_MOVES + j];
            self.p2[n] = m;
            if self.dfs2(nc, nu, ns, depth - 1, n + 1) {
                return true;
            }
            if self.sh.stop.load(Ordering::Relaxed) {
                return false;
            }
        }
        false
    }
}

/// Junta movimentos consecutivos da mesma face (inclusive atravessando faces opostas,
/// que comutam). So pode encurtar a sequencia.
pub fn simplify(moves: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(moves.len());
    for &orig in moves {
        let mut m = orig;
        loop {
            let mut hit = None;
            let mut k = out.len();
            while k > 0 {
                let prev = out[k - 1];
                if move_face(prev) == move_face(m) {
                    hit = Some(k - 1);
                    break;
                }
                if move_axis(prev) == move_axis(m) {
                    k -= 1; // face oposta: comuta, continua olhando para tras
                    continue;
                }
                break;
            }
            match hit {
                Some(i) => {
                    let p = ((out[i] % 3 + 1) + (m % 3 + 1)) % 4;
                    out.remove(i);
                    if p == 0 {
                        break; // cancelou totalmente
                    }
                    m = move_face(m) * 3 + (p - 1);
                }
                None => {
                    out.push(m);
                    break;
                }
            }
        }
    }
    out
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 12)
}

/// Resolve o cubo. `max_len` e o alvo (a busca para assim que atinge esse tamanho);
/// `timeout_ms` limita quanto tempo continuamos procurando algo melhor.
pub fn solve(
    cube: &CubieCube,
    t: &Tables,
    max_len: usize,
    timeout_ms: u64,
    threads: usize,
) -> Result<Solution, String> {
    if cube.is_solved() {
        return Ok(Solution {
            moves: Vec::new(),
            phase1: 0,
            nodes: 0,
            p1_sols: 0,
            threads: 0,
        });
    }
    let threads = threads.clamp(1, 12);
    let now = Instant::now();
    let sh = Shared {
        best: Mutex::new(None),
        best_len: AtomicUsize::new(usize::MAX),
        stop: AtomicBool::new(false),
        nodes: AtomicUsize::new(0),
        p1_sols: AtomicUsize::new(0),
        deadline: now + Duration::from_millis(timeout_ms),
        hard_deadline: now + Duration::from_millis(timeout_ms.max(15_000)),
        min_until: now + Duration::from_millis(timeout_ms.min(60)),
        target: max_len,
    };

    std::thread::scope(|s| {
        for tid in 0..threads {
            let shr = &sh;
            let tables = t;
            // Cada thread ataca uma variante diferente da mesma posicao:
            //   - um dos 3 eixos do cubo (muda qual eixo define o subgrupo G1)
            //   - direta ou invertida (arvore de busca completamente diferente)
            //   - ordem das faces na DFS
            let axis = (tid / 2) % 3;
            let inverted = tid % 2 == 1;
            let pi = &crate::facelet::ROT_PI[axis];
            let rotated = if axis == 0 {
                *cube
            } else {
                crate::facelet::rotate_cube(cube, pi, &crate::facelet::rotation_perm(pi))
            };
            let start_cube = if inverted { rotated.inverse() } else { rotated };
            let face_map = crate::facelet::inverse_face_map(pi);
            s.spawn(move || {
                let (order, order2) = make_orders(tid / 6 + axis);
                let mut se = Searcher {
                    t: tables,
                    sh: shr,
                    cube: start_cube,
                    p1: [0; MAX_P1_DEPTH + 1],
                    p2: [0; MAX_P2_DEPTH + 1],
                    p1_len: 0,
                    order,
                    order2,
                    counter: 0,
                    p1_solutions: 0,
                    inverted,
                    face_map,
                };
                se.run();
                shr.nodes.fetch_add(se.counter, Ordering::Relaxed);
                shr.p1_sols.fetch_add(se.p1_solutions, Ordering::Relaxed);
            });
        }
    });

    let guard = sh.best.lock().unwrap();
    let (raw, phase1) = match &*guard {
        Some(v) => v.clone(),
        None => return Err("nao consegui encontrar uma solucao no tempo disponivel".into()),
    };

    let moves = simplify(&raw);

    // Rede de seguranca: conferir que a sequencia realmente resolve o cubo.
    let mut c = *cube;
    for &m in &moves {
        c = c.multiply(&t.mc[m as usize]);
    }
    if !c.is_solved() {
        return Err("erro interno: a solucao encontrada nao resolve o cubo".into());
    }

    let phase1 = phase1.min(moves.len());
    Ok(Solution {
        moves,
        phase1,
        nodes: sh.nodes.load(Ordering::Relaxed),
        p1_sols: sh.p1_sols.load(Ordering::Relaxed),
        threads,
    })
}


