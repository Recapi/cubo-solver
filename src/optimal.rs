//! Solver otimo (estilo Korf): IDA* no espaco completo dos 18 movimentos.
//!
//! Heuristica: o maximo das distancias EXATAS de fase 1 nos tres eixos do cubo
//! (tabela de simetria). Cada uma e um limite inferior da distancia real,
//! porque qualquer solucao precisa, em particular, levar o cubo ao G1 daquele
//! eixo. A busca itera profundidades crescentes; uma iteracao que termina sem
//! solucao PROVA que nao existe solucao com aquele tamanho. O two-phase fornece
//! o limite superior; quando os dois se encontram, o otimo esta provado.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::coord::*;
use crate::cube::*;
use crate::facelet::{rotate_cube, rotation_perm, ROT_PI};
use crate::search::{self, SolveParams};
use crate::sym::BigP1;
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

/// Coordenadas de fase 1 do mesmo estado visto pelos tres eixos.
#[derive(Clone, Copy)]
struct Coords {
    a: [(u16, u16, u16); 3], // (twist, flip, slice) por eixo
}

struct Ctx<'a> {
    t: &'a Tables,
    big: &'a BigP1,
    /// axis_moves[eixo][m] = indice do movimento no referencial girado.
    axis_moves: [[u8; 18]; 3],
    root: CubieCube,
    stop: AtomicBool,
    deadline: Instant,
    nodes: AtomicUsize,
    cursor: AtomicUsize,
    found: Mutex<Option<Vec<u8>>>,
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

#[inline(always)]
fn step(ctx: &Ctx, co: &Coords, m: u8) -> Coords {
    let mut r = *co;
    for (a, slot) in r.a.iter_mut().enumerate() {
        let ma = ctx.axis_moves[a][m as usize] as usize;
        let (t, f, s) = *slot;
        *slot = (
            ctx.t.twist_move[t as usize * 18 + ma],
            ctx.t.flip_move[f as usize * 18 + ma],
            ctx.t.slice_move[s as usize * 18 + ma],
        );
    }
    r
}

#[inline(always)]
fn h3(ctx: &Ctx, co: &Coords) -> u8 {
    let mut h = 0u8;
    for &(t, f, s) in &co.a {
        h = h.max(ctx.big.h(t, f, s));
    }
    h
}

/// Coordenadas dos tres eixos de um estado qualquer (para a raiz e testes).
pub fn coords_of(cube: &CubieCube) -> [(u16, u16, u16); 3] {
    let mut out = [(0u16, 0u16, 0u16); 3];
    for a in 0..3 {
        let c = if a == 0 {
            *cube
        } else {
            let pi = &ROT_PI[a];
            rotate_cube(cube, pi, &rotation_perm(pi))
        };
        out[a] = (get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep));
    }
    out
}

struct Dfs<'a, 'b> {
    ctx: &'a Ctx<'b>,
    path: [u8; 24],
    counter: usize,
}

impl<'a, 'b> Dfs<'a, 'b> {
    #[inline]
    fn should_stop(&mut self) -> bool {
        if self.ctx.stop.load(Ordering::Relaxed) {
            return true;
        }
        self.counter += 1;
        if self.counter & 0x3FF == 0 && Instant::now() >= self.ctx.deadline {
            self.ctx.stop.store(true, Ordering::Relaxed);
            return true;
        }
        false
    }

    fn dfs(&mut self, co: &Coords, depth: usize, n: usize) -> bool {
        if self.should_stop() {
            return false;
        }
        let h = h3(self.ctx, co) as usize;
        if h > depth {
            return false;
        }
        if depth == 0 {
            // h == 0 nos tres eixos nao garante resolvido (as permutacoes dentro
            // de cada fatia ainda podem estar trocadas); confere de verdade.
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
            let next = step(self.ctx, co, m);
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

/// Pares (m1, m2) canonicos para dividir a raiz entre as threads.
fn root_tasks() -> Vec<(u8, u8)> {
    let mut tasks = Vec::with_capacity(270);
    for m1 in 0..18u8 {
        for m2 in 0..18u8 {
            let lf = move_face(m1);
            let f = move_face(m2);
            if f == lf || (lf >= 3 && f + 3 == lf) {
                continue;
            }
            tasks.push((m1, m2));
        }
    }
    tasks
}

/// Resolve com prova de otimalidade dentro do orcamento de tempo.
pub fn solve_optimal(
    cube: &CubieCube,
    t: &Tables,
    timeout_ms: u64,
    threads: usize,
) -> Result<OptimalOutcome, String> {
    let big = t
        .big
        .as_ref()
        .ok_or_else(|| "o modo otimo precisa da tabela de simetria (sem --no-bigtable)".to_string())?;
    if cube.is_solved() {
        return Ok(OptimalOutcome {
            moves: Vec::new(),
            lower_bound: 0,
            optimal: true,
            nodes: 0,
            threads: 0,
        });
    }
    let threads = threads.clamp(1, 12);
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms.clamp(500, 600_000));

    // Limite superior: o two-phase acha 17-19 rapido.
    let ub_budget = (timeout_ms / 4).clamp(200, 2000);
    let two = search::solve(
        cube,
        t,
        SolveParams {
            max_len: 20,
            target_len: 0,
            timeout_ms: ub_budget,
            min_ms: 0,
            threads,
        },
    )?;
    let mut best = two.moves;

    // Limite inferior inicial: d(c) = d(c^-1), entao vale o maximo dos dois.
    let axis_moves = axis_move_table();
    let inv = cube.inverse();
    let coords_c = Coords { a: coords_of(cube) };
    let coords_i = Coords { a: coords_of(&inv) };
    let probe = Ctx {
        t,
        big,
        axis_moves,
        root: *cube,
        stop: AtomicBool::new(false),
        deadline,
        nodes: AtomicUsize::new(0),
        cursor: AtomicUsize::new(0),
        found: Mutex::new(None),
    };
    let h_c = h3(&probe, &coords_c);
    let h_i = h3(&probe, &coords_i);
    let lb0 = h_c.max(h_i) as usize;

    // Busca na direcao com heuristica maior (poda melhor).
    let inverted = h_i > h_c;
    let (root_cube, root_coords) = if inverted { (inv, coords_i) } else { (*cube, coords_c) };

    let tasks = root_tasks();
    let mut completed = lb0.saturating_sub(1); // profundidades < lb0 provadas vazias
    let mut total_nodes = 0usize;
    let mut optimal = false;

    let mut d = lb0;
    while d < best.len() {
        if Instant::now() >= deadline {
            break;
        }
        let ctx = Ctx {
            t,
            big,
            axis_moves,
            root: root_cube,
            stop: AtomicBool::new(false),
            deadline,
            nodes: AtomicUsize::new(0),
            cursor: AtomicUsize::new(0),
            found: Mutex::new(None),
        };

        if d < 2 {
            // profundidades triviais: uma thread da conta
            let mut w = Dfs { ctx: &ctx, path: [0; 24], counter: 0 };
            w.dfs(&root_coords, d, 0);
            total_nodes += w.counter;
        } else {
            std::thread::scope(|sc| {
                for _ in 0..threads {
                    let ctx = &ctx;
                    let tasks = &tasks;
                    let root_coords = &root_coords;
                    sc.spawn(move || {
                        let mut w = Dfs { ctx, path: [0; 24], counter: 0 };
                        loop {
                            let i = ctx.cursor.fetch_add(1, Ordering::Relaxed);
                            if i >= tasks.len() || ctx.stop.load(Ordering::Relaxed) {
                                break;
                            }
                            let (m1, m2) = tasks[i];
                            let c1 = step(ctx, root_coords, m1);
                            let c2 = step(ctx, &c1, m2);
                            w.path[0] = m1;
                            w.path[1] = m2;
                            if w.dfs(&c2, d - 2, 2) {
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
            // solucao com exatamente d movimentos; profundidades menores ja
            // foram provadas vazias -> otima
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
            break; // tempo esgotado no meio da iteracao: ela nao conta como prova
        }
        completed = d;
        d += 1;
    }

    let lower_bound = if optimal { best.len() } else { (completed + 1).min(best.len()) };
    let optimal = optimal || lower_bound >= best.len();

    // Rede de seguranca
    let mut c = *cube;
    for &m in &best {
        c = c.multiply(&t.mc[m as usize]);
    }
    if !c.is_solved() {
        return Err("erro interno: a solucao otima nao resolve o cubo".into());
    }

    Ok(OptimalOutcome {
        moves: best,
        lower_bound,
        optimal,
        nodes: total_nodes + two.nodes,
        threads,
    })
}
