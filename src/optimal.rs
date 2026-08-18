//! Solver otimo (estilo Korf): IDA* no espaco completo dos 18 movimentos.
//!
//! Heuristica preferida: a tabela X (fase 1 refinada com a identidade das
//! arestas da fatia, distancia exata via mod 3) nos tres eixos do cubo. Sem
//! ela, cai para a distancia exata de fase 1 (tabela de simetria). Ambas sao
//! limites inferiores da distancia real; a busca itera profundidades
//! crescentes e cada iteracao vazia PROVA que nao existe solucao daquele
//! tamanho. O two-phase fornece o limite superior; quando os dois se
//! encontram, o otimo esta provado.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::coord::*;
use crate::cube::*;
use crate::facelet::{rotate_cube, rotation_perm, ROT_PI};
use crate::search::{self, SolveParams};
use crate::tables::Tables;

pub struct OptimalOutcome {
    pub moves: Vec<u8>,
    /// Provado: nao existe solucao com menos de `lower_bound` movimentos.
    pub lower_bound: usize,
    /// `lower_bound == moves.len()`: a solucao e otima, com prova.
    pub optimal: bool,
    pub nodes: usize,
    pub threads: usize,
}

/// Progresso/cancelamento compartilhado com quem chamou (API de jobs).
pub struct SolveCtrl {
    pub cancel: AtomicBool,
    pub lower_bound: AtomicUsize,
    pub best_len: AtomicUsize,
    pub nodes: AtomicUsize,
}

impl SolveCtrl {
    pub fn new() -> SolveCtrl {
        SolveCtrl {
            cancel: AtomicBool::new(false),
            lower_bound: AtomicUsize::new(0),
            best_len: AtomicUsize::new(0),
            nodes: AtomicUsize::new(0),
        }
    }
}

impl Default for SolveCtrl {
    fn default() -> Self {
        Self::new()
    }
}

pub fn axis_move_table() -> [[u8; 18]; 3] {
    let mut am = [[0u8; 18]; 3];
    for a in 0..3 {
        for m in 0..18 {
            am[a][m] = (ROT_PI[a][m / 3] * 3 + m % 3) as u8;
        }
    }
    am
}

/// Coordenadas de fase 1 dos tres eixos de um estado (para testes e raiz).
pub fn coords_of(cube: &CubieCube) -> [(u16, u16, u16); 3] {
    let mut out = [(0u16, 0u16, 0u16); 3];
    for a in 0..3 {
        let c = axis_view(cube, a);
        out[a] = (get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep));
    }
    out
}

fn axis_view(cube: &CubieCube, a: usize) -> CubieCube {
    if a == 0 {
        *cube
    } else {
        let pi = &ROT_PI[a];
        rotate_cube(cube, pi, &rotation_perm(pi))
    }
}

// ---------------------------------------------------------------------------
// Heuristicas intercambiaveis
// ---------------------------------------------------------------------------

trait Heur: Sync {
    type St: Copy + Send + Sync;
    fn root(&self, cube: &CubieCube) -> Self::St;
    fn step(&self, s: &Self::St, m: u8) -> Self::St;
    fn h(&self, s: &Self::St) -> u8;
}

/// Fallback: distancia exata de fase 1 nos 3 eixos (tabela de simetria).
struct P1Heur<'a> {
    t: &'a Tables,
    big: &'a crate::sym::BigP1,
    am: [[u8; 18]; 3],
}

impl<'a> Heur for P1Heur<'a> {
    type St = [(u16, u16, u16); 3];

    fn root(&self, cube: &CubieCube) -> Self::St {
        coords_of(cube)
    }

    #[inline(always)]
    fn step(&self, s: &Self::St, m: u8) -> Self::St {
        let mut r = *s;
        for (a, slot) in r.iter_mut().enumerate() {
            let ma = self.am[a][m as usize] as usize;
            let (t, f, sl) = *slot;
            *slot = (
                self.t.twist_move[t as usize * 18 + ma],
                self.t.flip_move[f as usize * 18 + ma],
                self.t.slice_move[sl as usize * 18 + ma],
            );
        }
        r
    }

    #[inline(always)]
    fn h(&self, s: &Self::St) -> u8 {
        let mut h = 0u8;
        for &(t, f, sl) in s {
            h = h.max(self.big.h(t, f, sl));
        }
        h
    }
}

/// Preferida: tabela X (com identidade das arestas da fatia), distancia exata
/// carregada incrementalmente a partir do mod 3.
#[derive(Clone, Copy)]
struct XAx {
    t: u16,
    f: u16,
    e: u16,
    exact: u8,
    m3: u8,
}

struct XHeur<'a> {
    t: &'a Tables,
    x: &'a crate::xtable::BigX,
    am: [[u8; 18]; 3],
}

impl<'a> Heur for XHeur<'a> {
    type St = [XAx; 3];

    fn root(&self, cube: &CubieCube) -> Self::St {
        let mut out = [XAx { t: 0, f: 0, e: 0, exact: 0, m3: 0 }; 3];
        for a in 0..3 {
            let c = axis_view(cube, a);
            let (tw, f, e) = (get_twist(&c.co), get_flip(&c.eo), get_epos(&c.ep));
            out[a] = XAx {
                t: tw,
                f,
                e,
                exact: self.x.exact(self.t, tw, f, e),
                m3: self.x.m3(tw, f, e),
            };
        }
        out
    }

    #[inline(always)]
    fn step(&self, s: &Self::St, m: u8) -> Self::St {
        let mut r = *s;
        for (a, ax) in r.iter_mut().enumerate() {
            let ma = self.am[a][m as usize] as usize;
            let t2 = self.t.twist_move[ax.t as usize * 18 + ma];
            let f2 = self.t.flip_move[ax.f as usize * 18 + ma];
            let e2 = self.x.epos_move[ax.e as usize * 18 + ma];
            let m3 = self.x.m3(t2, f2, e2);
            // (m3 - anterior) mod 3: 1 = +1, 2 = -1, 0 = igual
            let d = (m3 + 3 - ax.m3) % 3;
            let exact = match d {
                1 => ax.exact + 1,
                2 => ax.exact - 1,
                _ => ax.exact,
            };
            *ax = XAx { t: t2, f: f2, e: e2, exact, m3 };
        }
        r
    }

    #[inline(always)]
    fn h(&self, s: &Self::St) -> u8 {
        s[0].exact.max(s[1].exact).max(s[2].exact)
    }
}

// ---------------------------------------------------------------------------
// IDA* com prova, generico na heuristica
// ---------------------------------------------------------------------------

struct Ctx<'a, H: Heur> {
    heur: &'a H,
    t: &'a Tables,
    root: CubieCube,
    stop: AtomicBool,
    deadline: Instant,
    nodes: AtomicUsize,
    cursor: AtomicUsize,
    found: Mutex<Option<Vec<u8>>>,
    ctrl: Option<&'a SolveCtrl>,
}

struct Dfs<'a, 'b, H: Heur> {
    ctx: &'a Ctx<'b, H>,
    path: [u8; 24],
    counter: usize,
}

impl<'a, 'b, H: Heur> Dfs<'a, 'b, H> {
    #[inline]
    fn should_stop(&mut self) -> bool {
        if self.ctx.stop.load(Ordering::Relaxed) {
            return true;
        }
        self.counter += 1;
        if self.counter & 0x3FF == 0 {
            if let Some(c) = self.ctx.ctrl {
                c.nodes.fetch_add(0x400, Ordering::Relaxed);
                if c.cancel.load(Ordering::Relaxed) {
                    self.ctx.stop.store(true, Ordering::Relaxed);
                    return true;
                }
            }
            if Instant::now() >= self.ctx.deadline {
                self.ctx.stop.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    fn dfs(&mut self, s: &H::St, depth: usize, n: usize) -> bool {
        if self.should_stop() {
            return false;
        }
        if self.ctx.heur.h(s) as usize > depth {
            return false;
        }
        if depth == 0 {
            // h == 0 nao garante resolvido (permutacoes de cantos/arestas U/D
            // podem estar trocadas); confere de verdade.
            let mut c = self.ctx.root;
            for &m in &self.path[..n] {
                c = c.multiply(&self.ctx.t.mc[m as usize]);
            }
            if c.is_solved() {
                let mut g = self.ctx.found.lock().unwrap();
                if g.is_none() {
                    *g = Some(self.path[..n].to_vec());
                    self.ctx.stop.store(true, Ordering::Relaxed);
                }
                return true;
            }
            return false;
        }
        for m in 0..18u8 {
            if n > 0 {
                let lf = move_face(self.path[n - 1]);
                let f = move_face(m);
                if f == lf || (lf >= 3 && f + 3 == lf) {
                    continue;
                }
            }
            let next = self.ctx.heur.step(s, m);
            self.path[n] = m;
            if self.dfs(&next, depth - 1, n + 1) {
                return true;
            }
            if self.ctx.stop.load(Ordering::Relaxed) {
                return false;
            }
        }
        false
    }
}

/// Prefixos canonicos de 3 movimentos para dividir a raiz (~4050 subarvores;
/// quanto mais finas, menos threads ociosas no fim de cada iteracao).
fn root_tasks() -> Vec<[u8; 3]> {
    let ok = |prev: u8, m: u8| {
        let lf = move_face(prev);
        let f = move_face(m);
        f != lf && !(lf >= 3 && f + 3 == lf)
    };
    let mut tasks = Vec::with_capacity(4050);
    for m1 in 0..18u8 {
        for m2 in 0..18u8 {
            if !ok(m1, m2) {
                continue;
            }
            for m3 in 0..18u8 {
                if ok(m2, m3) {
                    tasks.push([m1, m2, m3]);
                }
            }
        }
    }
    tasks
}

struct RunResult {
    best: Vec<u8>,
    lower_bound: usize,
    optimal: bool,
    nodes: usize,
}

#[allow(clippy::too_many_arguments)]
fn run_proof<H: Heur>(
    heur: &H,
    t: &Tables,
    cube: &CubieCube,
    mut best: Vec<u8>,
    deadline: Instant,
    threads: usize,
    ctrl: Option<&SolveCtrl>,
) -> RunResult {
    // d(c) = d(c^-1): busca na direcao com heuristica maior e usa o maximo
    // dos dois como limite inferior inicial.
    let inv = cube.inverse();
    let st_c = heur.root(cube);
    let st_i = heur.root(&inv);
    let h_c = heur.h(&st_c);
    let h_i = heur.h(&st_i);
    let lb0 = h_c.max(h_i) as usize;
    let inverted = h_i > h_c;
    let (root_cube, root_st) = if inverted { (inv, st_i) } else { (*cube, st_c) };

    if let Some(c) = ctrl {
        c.lower_bound.store(lb0.min(best.len()), Ordering::Relaxed);
    }

    // Subarvores mais promissoras primeiro: a iteracao final acha a solucao
    // mais cedo (nas iteracoes de prova a ordem nao importa).
    let mut tasks = root_tasks();
    tasks.sort_by_cached_key(|&[m1, m2, m3]| {
        let s = heur.step(&heur.step(&heur.step(&root_st, m1), m2), m3);
        heur.h(&s)
    });

    let mut completed = lb0.saturating_sub(1);
    let mut total_nodes = 0usize;
    let mut optimal = false;
    let mut d = lb0;
    while d < best.len() {
        if Instant::now() >= deadline
            || ctrl.map(|c| c.cancel.load(Ordering::Relaxed)).unwrap_or(false)
        {
            break;
        }
        if let Some(c) = ctrl {
            c.lower_bound.store(d.min(best.len()), Ordering::Relaxed);
        }
        let ctx = Ctx {
            heur,
            t,
            root: root_cube,
            stop: AtomicBool::new(false),
            deadline,
            nodes: AtomicUsize::new(0),
            cursor: AtomicUsize::new(0),
            found: Mutex::new(None),
            ctrl,
        };

        if d < 3 {
            let mut w = Dfs { ctx: &ctx, path: [0; 24], counter: 0 };
            w.dfs(&root_st, d, 0);
            total_nodes += w.counter;
        } else {
            std::thread::scope(|sc| {
                for _ in 0..threads {
                    let ctx = &ctx;
                    let tasks = &tasks;
                    let root_st = &root_st;
                    sc.spawn(move || {
                        let mut w = Dfs { ctx, path: [0; 24], counter: 0 };
                        loop {
                            let i = ctx.cursor.fetch_add(1, Ordering::Relaxed);
                            if i >= tasks.len() || ctx.stop.load(Ordering::Relaxed) {
                                break;
                            }
                            let [m1, m2, m3] = tasks[i];
                            let s = ctx.heur.step(
                                &ctx.heur.step(&ctx.heur.step(root_st, m1), m2),
                                m3,
                            );
                            w.path[0] = m1;
                            w.path[1] = m2;
                            w.path[2] = m3;
                            if w.dfs(&s, d - 3, 3) {
                                break;
                            }
                        }
                        ctx.nodes.fetch_add(w.counter, Ordering::Relaxed);
                    });
                }
            });
            total_nodes += ctx.nodes.load(Ordering::Relaxed);
        }

        let found = ctx.found.lock().unwrap().take();
        if let Some(mut mv) = found {
            if inverted {
                mv.reverse();
                for m in mv.iter_mut() {
                    *m = move_inverse(*m);
                }
            }
            best = mv;
            completed = d.saturating_sub(1);
            optimal = true;
            break;
        }
        if ctx.stop.load(Ordering::Relaxed) {
            break; // tempo/cancelamento no meio da iteracao: nao conta como prova
        }
        completed = d;
        d += 1;
    }

    let lower_bound = if optimal { best.len() } else { (completed + 1).min(best.len()) };
    let optimal = optimal || lower_bound >= best.len();
    RunResult { best, lower_bound, optimal, nodes: total_nodes }
}

/// Resolve com prova de otimalidade dentro do orcamento de tempo.
pub fn solve_optimal(
    cube: &CubieCube,
    t: &Tables,
    timeout_ms: u64,
    threads: usize,
    ctrl: Option<&SolveCtrl>,
) -> Result<OptimalOutcome, String> {
    if t.big.is_none() && t.bigx.is_none() {
        return Err("o modo otimo precisa das tabelas de simetria (sem --no-bigtable)".into());
    }
    if cube.is_solved() {
        return Ok(OptimalOutcome {
            moves: Vec::new(),
            lower_bound: 0,
            optimal: true,
            nodes: 0,
            threads: 0,
        });
    }
    let threads = threads.clamp(1, 64);
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms.clamp(500, 600_000));

    // Limite superior: o two-phase acha 17-19 rapido.
    let ub_budget = (timeout_ms / 4).clamp(200, 2000);
    let two = search::solve(
        cube,
        t,
        SolveParams { max_len: 20, target_len: 0, timeout_ms: ub_budget, min_ms: 0, threads },
    )?;
    if let Some(c) = ctrl {
        c.best_len.store(two.moves.len(), Ordering::Relaxed);
        c.nodes.fetch_add(two.nodes, Ordering::Relaxed);
    }

    let am = axis_move_table();
    let r = if let Some(x) = &t.bigx {
        let heur = XHeur { t, x, am };
        run_proof(&heur, t, cube, two.moves, deadline, threads, ctrl)
    } else {
        let heur = P1Heur { t, big: t.big.as_ref().unwrap(), am };
        run_proof(&heur, t, cube, two.moves, deadline, threads, ctrl)
    };

    if let Some(c) = ctrl {
        c.best_len.store(r.best.len(), Ordering::Relaxed);
        c.lower_bound.store(r.lower_bound, Ordering::Relaxed);
    }

    // Rede de seguranca
    let mut c = *cube;
    for &m in &r.best {
        c = c.multiply(&t.mc[m as usize]);
    }
    if !c.is_solved() {
        return Err("erro interno: a solucao otima nao resolve o cubo".into());
    }

    Ok(OptimalOutcome {
        moves: r.best,
        lower_bound: r.lower_bound,
        optimal: r.optimal,
        nodes: r.nodes + two.nodes,
        threads,
    })
}
