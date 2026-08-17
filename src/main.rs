mod coord;
mod cube;
mod facelet;
mod optimal;
mod search;
mod sym;
mod tables;
mod xtable;

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use cube::{move_face, move_name, CubieCube, MOVE_NAMES, N_MOVES, SOLVED};
use tables::Tables;

const INDEX_HTML: &str = include_str!("../static/index.html");
const STYLE_CSS: &str = include_str!("../static/style.css");
const APP_JS: &str = include_str!("../static/app.js");

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new() -> Rng {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Rng(n | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Converte notacao ("R U R' U2") em indices de movimento.
fn parse_moves(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for tok in s.split_whitespace() {
        // aceita apostrofo tipografico (U+2019) e crase no lugar de '
        let t = tok.replace('\u{2019}', "'").replace('`', "'");
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        match MOVE_NAMES.iter().position(|&m| m.eq_ignore_ascii_case(t)) {
            Some(i) => out.push(i as u8),
            None => {
                // Aceita tambem "R3" como R'
                let up = t.to_uppercase();
                let alt = up.replace('3', "'");
                match MOVE_NAMES.iter().position(|&m| m == alt) {
                    Some(i) => out.push(i as u8),
                    None => return Err(format!("movimento desconhecido: \"{}\"", tok)),
                }
            }
        }
    }
    Ok(out)
}

fn apply_moves(c: &CubieCube, moves: &[u8], t: &Tables) -> CubieCube {
    let mut r = *c;
    for &m in moves {
        r = r.multiply(&t.mc[m as usize]);
    }
    r
}

fn random_scramble(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(len);
    while out.len() < len {
        let m = rng.below(N_MOVES as u64) as u8;
        if let Some(&last) = out.last() {
            if move_face(last) == move_face(m) {
                continue;
            }
            if out.len() >= 2 {
                let prev = out[out.len() - 2];
                // evita padroes tipo R L R
                if move_face(prev) == move_face(m) && (move_face(last) % 3) == (move_face(m) % 3) {
                    continue;
                }
            }
        }
        out.push(m);
    }
    out
}

fn notation(moves: &[u8]) -> Vec<String> {
    moves.iter().map(|&m| move_name(m).to_string()).collect()
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SolveReq {
    facelets: String,
    /// Tamanho maximo aceitavel (1..=30, padrao 20).
    #[serde(default)]
    max_len: Option<usize>,
    /// Para ao achar solucao com ate este tamanho; 0 = usar o tempo todo.
    #[serde(default)]
    target_len: Option<usize>,
    /// Tempo maximo de busca em ms (50..=30000, padrao 4000).
    #[serde(default)]
    timeout_ms: Option<u64>,
    /// Esforco minimo em ms antes de aceitar parar no alvo (padrao 60).
    #[serde(default)]
    min_ms: Option<u64>,
    /// Numero de threads (1..=12, padrao: nucleos da maquina).
    #[serde(default)]
    threads: Option<usize>,
    /// true = modo otimo: prova que nao existe solucao menor (pode demorar).
    #[serde(default)]
    optimal: Option<bool>,
}

#[derive(Serialize)]
struct SolveResp {
    solution: Vec<String>,
    notation: String,
    length: usize,
    phase1: usize,
    phase2: usize,
    time_ms: u128,
    nodes: usize,
    solutions: usize,
    threads: usize,
    /// So no modo otimo: a solucao e provadamente otima?
    #[serde(skip_serializing_if = "Option::is_none")]
    optimal: Option<bool>,
    /// So no modo otimo: provado que nao existe solucao com menos que isto.
    #[serde(skip_serializing_if = "Option::is_none")]
    lower_bound: Option<usize>,
    states: Vec<String>,
}

#[derive(Deserialize)]
struct ScrambleReq {
    #[serde(default)]
    length: Option<usize>,
}

#[derive(Serialize)]
struct ScrambleResp {
    facelets: String,
    scramble: Vec<String>,
    notation: String,
}

#[derive(Deserialize)]
struct ApplyReq {
    #[serde(default)]
    facelets: Option<String>,
    moves: String,
}

#[derive(Serialize)]
struct ApplyResp {
    facelets: String,
    moves: Vec<String>,
}

fn bad_request(msg: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
}

type ApiError = (StatusCode, Json<serde_json::Value>);

#[allow(clippy::too_many_arguments)]
fn build_solve_resp(
    t: &Tables,
    cube: &CubieCube,
    moves: &[u8],
    phase1: usize,
    nodes: usize,
    solutions: usize,
    threads: usize,
    optimal: Option<bool>,
    lower_bound: Option<usize>,
    time_ms: u128,
) -> SolveResp {
    let mut states = Vec::with_capacity(moves.len() + 1);
    let mut c = *cube;
    states.push(facelet::to_facelets(&c));
    for &m in moves {
        c = c.multiply(&t.mc[m as usize]);
        states.push(facelet::to_facelets(&c));
    }
    let names = notation(moves);
    SolveResp {
        notation: names.join(" "),
        length: moves.len(),
        phase1,
        phase2: moves.len() - phase1,
        time_ms,
        nodes,
        solutions,
        threads,
        optimal,
        lower_bound,
        solution: names,
        states,
    }
}

// ---------------------------------------------------------------------------
// Estado do servidor: tabelas + jobs do modo otimo (progresso e cancelamento)
// ---------------------------------------------------------------------------

struct OptJob {
    ctrl: optimal::SolveCtrl,
    started: Instant,
    done: std::sync::atomic::AtomicBool,
    result: std::sync::Mutex<Option<Result<SolveResp, String>>>,
}

#[derive(Clone)]
struct AppState {
    tables: Arc<Tables>,
    jobs: Arc<std::sync::Mutex<std::collections::HashMap<u64, Arc<OptJob>>>>,
    next_job: Arc<std::sync::atomic::AtomicU64>,
}

async fn api_solve(
    State(st): State<AppState>,
    Json(req): Json<SolveReq>,
) -> Result<Json<SolveResp>, ApiError> {
    let t = st.tables;
    let cube = facelet::to_cubie(&req.facelets).map_err(bad_request)?;
    let max_len = req.max_len.unwrap_or(20).clamp(1, 30);
    let want_optimal = req.optimal.unwrap_or(false);
    let params = search::SolveParams {
        max_len,
        target_len: req.target_len.unwrap_or(max_len).min(max_len),
        timeout_ms: req
            .timeout_ms
            .unwrap_or(if want_optimal { 60_000 } else { 4000 })
            .clamp(50, 600_000),
        min_ms: req.min_ms.unwrap_or(60),
        threads: req.threads.unwrap_or_else(search::default_threads).clamp(1, 12),
    };

    let res = tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        if want_optimal {
            optimal::solve_optimal(&cube, &t, params.timeout_ms, params.threads, None).map(|o| {
                build_solve_resp(
                    &t,
                    &cube,
                    &o.moves,
                    o.moves.len(),
                    o.nodes,
                    1,
                    o.threads,
                    Some(o.optimal),
                    Some(o.lower_bound),
                    start.elapsed().as_millis(),
                )
            })
        } else {
            search::solve(&cube, &t, params).map(|s| {
                build_solve_resp(
                    &t,
                    &cube,
                    &s.moves,
                    s.phase1,
                    s.nodes,
                    s.solutions,
                    s.threads,
                    None,
                    None,
                    start.elapsed().as_millis(),
                )
            })
        }
    })
    .await
    .map_err(|e| bad_request(format!("falha interna: {e}")))?;

    res.map(Json).map_err(bad_request)
}

// ---------------------------------------------------------------------------
// Jobs do modo otimo: inicia em segundo plano, consulta progresso, cancela
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OptStartReq {
    facelets: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    threads: Option<usize>,
}

async fn api_opt_start(
    State(st): State<AppState>,
    Json(req): Json<OptStartReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cube = facelet::to_cubie(&req.facelets).map_err(bad_request)?;
    let timeout_ms = req.timeout_ms.unwrap_or(60_000).clamp(500, 600_000);
    let threads = req.threads.unwrap_or_else(search::default_threads).clamp(1, 12);

    // limpeza de jobs concluidos e esquecidos
    {
        let mut jobs = st.jobs.lock().unwrap();
        jobs.retain(|_, j| {
            !(j.done.load(std::sync::atomic::Ordering::Relaxed)
                && j.started.elapsed().as_secs() > 3600)
        });
    }

    let id = st.next_job.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let job = Arc::new(OptJob {
        ctrl: optimal::SolveCtrl::new(),
        started: Instant::now(),
        done: std::sync::atomic::AtomicBool::new(false),
        result: std::sync::Mutex::new(None),
    });
    st.jobs.lock().unwrap().insert(id, job.clone());

    let tables = st.tables.clone();
    tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        let out = optimal::solve_optimal(&cube, &tables, timeout_ms, threads, Some(&job.ctrl))
            .map(|o| {
                build_solve_resp(
                    &tables,
                    &cube,
                    &o.moves,
                    o.moves.len(),
                    o.nodes,
                    1,
                    o.threads,
                    Some(o.optimal),
                    Some(o.lower_bound),
                    start.elapsed().as_millis(),
                )
            });
        *job.result.lock().unwrap() = Some(out);
        job.done.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    Ok(Json(serde_json::json!({ "job": id })))
}

async fn api_opt_status(
    State(st): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let job = st
        .jobs
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| bad_request("job desconhecido".into()))?;
    use std::sync::atomic::Ordering::Relaxed;
    let done = job.done.load(Relaxed);
    let mut v = serde_json::json!({
        "done": done,
        "elapsed_ms": job.started.elapsed().as_millis() as u64,
        "lower_bound": job.ctrl.lower_bound.load(Relaxed),
        "best_len": job.ctrl.best_len.load(Relaxed),
        "nodes": job.ctrl.nodes.load(Relaxed),
    });
    if done {
        let guard = job.result.lock().unwrap();
        match guard.as_ref() {
            Some(Ok(r)) => {
                v["result"] = serde_json::to_value(r)
                    .map_err(|e| bad_request(format!("falha interna: {e}")))?;
            }
            Some(Err(e)) => v["error"] = serde_json::Value::String(e.clone()),
            None => {}
        }
        drop(guard);
        st.jobs.lock().unwrap().remove(&id);
    }
    Ok(Json(v))
}

async fn api_opt_cancel(
    State(st): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<serde_json::Value> {
    if let Some(job) = st.jobs.lock().unwrap().get(&id) {
        job.ctrl.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn api_scramble(
    State(st): State<AppState>,
    Json(req): Json<ScrambleReq>,
) -> Json<ScrambleResp> {
    let t = st.tables;
    let len = req.length.unwrap_or(25).clamp(1, 100);
    let mut rng = Rng::new();
    let moves = random_scramble(&mut rng, len);
    let c = apply_moves(&SOLVED, &moves, &t);
    let names = notation(&moves);
    Json(ScrambleResp {
        facelets: facelet::to_facelets(&c),
        notation: names.join(" "),
        scramble: names,
    })
}

async fn api_apply(
    State(st): State<AppState>,
    Json(req): Json<ApplyReq>,
) -> Result<Json<ApplyResp>, ApiError> {
    let t = st.tables;
    let base = match req.facelets.as_deref() {
        Some(f) if !f.trim().is_empty() => facelet::to_cubie(f).map_err(bad_request)?,
        _ => SOLVED,
    };
    let moves = parse_moves(&req.moves).map_err(bad_request)?;
    let c = apply_moves(&base, &moves, &t);
    Ok(Json(ApplyResp {
        facelets: facelet::to_facelets(&c),
        moves: notation(&moves),
    }))
}

async fn page_index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], INDEX_HTML)
}
async fn page_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE_CSS)
}
async fn page_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        APP_JS,
    )
}

// ---------------------------------------------------------------------------
// Modo CLI (benchmark / solve avulso)
// ---------------------------------------------------------------------------

fn env_num<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// BENCH_SEED fixa a sequencia de cubos: permite comparar A/B com os mesmos casos.
fn bench_rng() -> Rng {
    match std::env::var("BENCH_SEED").ok().and_then(|v| v.parse::<u64>().ok()) {
        Some(s) => Rng(s | 1),
        None => Rng::new(),
    }
}

fn bench_params() -> search::SolveParams {
    search::SolveParams {
        max_len: env_num("BENCH_MAX", 20),
        target_len: env_num("BENCH_TARGET", 20),
        timeout_ms: env_num("BENCH_TIMEOUT", 4000),
        min_ms: env_num("BENCH_MIN", 60),
        threads: env_num("BENCH_THREADS", search::default_threads()),
    }
}

fn run_bench(t: &Tables, n: usize) {
    let mut rng = bench_rng();
    let mut total_len = 0usize;
    let mut total_ms = 0u128;
    let mut worst = 0usize;
    let mut worst_ms = 0u128;
    let mut hist = [0usize; 31];
    let params = bench_params();
    println!(
        "Resolvendo {n} cubos aleatorios ({} threads, alvo {}, max {}, {} ms, esforco minimo {} ms)...",
        params.threads, params.target_len, params.max_len, params.timeout_ms, params.min_ms
    );
    let mut total_nodes = 0usize;
    for i in 0..n {
        let scr = random_scramble(&mut rng, 25);
        let cube = apply_moves(&SOLVED, &scr, t);
        let start = Instant::now();
        let sol = search::solve(&cube, t, params)
            .unwrap_or_else(|e| panic!("falhou no caso {i}: {e}"));
        let ms = start.elapsed().as_millis();
        // verificacao independente
        let end = apply_moves(&cube, &sol.moves, t);
        assert!(end.is_solved(), "solucao invalida no caso {i}");
        if std::env::var("BENCH_VERBOSE").is_ok() {
            println!(
                "  caso {i}: {} movimentos, {ms} ms, {} nos, {} solucoes de fase 1",
                sol.moves.len(),
                sol.nodes,
                sol.p1_sols
            );
        }
        total_len += sol.moves.len();
        total_ms += ms;
        total_nodes += sol.nodes;
        hist[sol.moves.len()] += 1;
        if sol.moves.len() > worst {
            worst = sol.moves.len();
        }
        if ms > worst_ms {
            worst_ms = ms;
        }
    }
    println!("media  : {:.2} movimentos", total_len as f64 / n as f64);
    println!("maximo : {worst} movimentos");
    println!("tempo  : {:.1} ms em media, {worst_ms} ms no pior caso", total_ms as f64 / n as f64);
    println!("nos    : {:.2} M em media", total_nodes as f64 / n as f64 / 1e6);
    print!("distribuicao:");
    for (l, &c) in hist.iter().enumerate() {
        if c > 0 {
            print!(" {l}:{c}");
        }
    }
    println!();
}

fn run_bench_optimal(t: &Tables, n: usize) {
    let mut rng = bench_rng();
    let timeout: u64 = env_num("BENCH_TIMEOUT", 120_000);
    println!("Resolvendo {n} cubos aleatorios no modo OTIMO (ate {timeout} ms cada)...");
    let mut proven = 0usize;
    for i in 0..n {
        let scr = random_scramble(&mut rng, 25);
        let cube = apply_moves(&SOLVED, &scr, t);
        let start = Instant::now();
        let o = optimal::solve_optimal(&cube, t, timeout, search::default_threads(), None)
            .unwrap_or_else(|e| panic!("caso {i}: {e}"));
        let end = apply_moves(&cube, &o.moves, t);
        assert!(end.is_solved(), "caso {i}: solucao invalida");
        if o.optimal {
            proven += 1;
        }
        println!(
            "  caso {i}: {} movimentos, {} em {:.1} s, {:.0} M nos",
            o.moves.len(),
            if o.optimal { "OTIMO provado".to_string() } else { format!("provado >= {}", o.lower_bound) },
            start.elapsed().as_secs_f64(),
            o.nodes as f64 / 1e6
        );
    }
    println!("provados: {proven}/{n}");
}

// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    print!("Gerando tabelas basicas... ");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let t0 = Instant::now();
    let mut tables = Tables::build();
    println!("pronto em {} ms", t0.elapsed().as_millis());

    // Tabela grande da fase 1 (distancia exata via simetria, ~140 MB de RAM).
    // Desligavel com --no-bigtable ou NO_BIGTABLE=1 para maquinas com pouca RAM.
    let no_big = args.iter().any(|a| a == "--no-bigtable")
        || std::env::var("NO_BIGTABLE").map(|v| v == "1").unwrap_or(false);
    if no_big {
        println!("Tabela de simetria da fase 1 desligada (--no-bigtable).");
    } else {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let cache1 = exe_dir.as_ref().map(|d| d.join("p1sym.cache"));
        let cached1 = cache1.as_deref().map(|p| p.exists()).unwrap_or(false);
        if cached1 {
            print!("Carregando tabela de simetria da fase 1 (~140 MB)... ");
        } else {
            println!("Gerando tabela de simetria da fase 1 (~140 MB, so na primeira vez):");
        }
        let _ = std::io::stdout().flush();
        let t1 = Instant::now();
        tables.big = Some(sym::BigP1::load_or_build(&tables, cache1.as_deref(), !cached1));
        println!("pronto em {:.1} s", t1.elapsed().as_secs_f64());

        let cache2 = exe_dir.as_ref().map(|d| d.join("p2sym.cache"));
        let cached2 = cache2.as_deref().map(|p| p.exists()).unwrap_or(false);
        if cached2 {
            print!("Carregando tabela de simetria da fase 2 (~112 MB)... ");
        } else {
            println!("Gerando tabela de simetria da fase 2 (~112 MB, so na primeira vez):");
        }
        let _ = std::io::stdout().flush();
        let t2 = Instant::now();
        tables.big2 = Some(sym::BigP2::load_or_build(&tables, cache2.as_deref(), !cached2));
        println!("pronto em {:.1} s", t2.elapsed().as_secs_f64());

        // Tabela X do modo otimo: pesada (~930 MB de RAM; ~2 GB durante a
        // primeira geracao). NO_XTABLE=1 ou --no-xtable desligam so ela.
        let no_x = args.iter().any(|a| a == "--no-xtable")
            || std::env::var("NO_XTABLE").map(|v| v == "1").unwrap_or(false);
        if no_x {
            println!("Tabela X do modo otimo desligada (--no-xtable).");
        } else {
            let cachex = exe_dir.as_ref().map(|d| d.join("p15sym.cache"));
            let cachedx = cachex.as_deref().map(|p| p.exists()).unwrap_or(false);
            if cachedx {
                print!("Carregando tabela X do modo otimo (~930 MB)... ");
            } else {
                println!("Gerando tabela X do modo otimo (~930 MB, alguns minutos so na primeira vez):");
            }
            let _ = std::io::stdout().flush();
            let tx = Instant::now();
            tables.bigx = Some(xtable::BigX::load_or_build(&tables, cachex.as_deref(), !cachedx));
            println!("pronto em {:.1} s", tx.elapsed().as_secs_f64());
        }
    }
    let tables = Arc::new(tables);

    // --- modos CLI ---
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let n = args
            .get(pos + 1)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(100);
        run_bench(&tables, n);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--bench-optimal") {
        let n = args
            .get(pos + 1)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(3);
        run_bench_optimal(&tables, n);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--solve") {
        let input = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("");
        match facelet::to_cubie(input) {
            Ok(c) => match search::solve(&c, &tables, bench_params()) {
                Ok(s) => println!("{} ({} movimentos)", notation(&s.moves).join(" "), s.moves.len()),
                Err(e) => println!("erro: {e}"),
            },
            Err(e) => println!("estado invalido: {e}"),
        }
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--scramble") {
        let seq = args.get(pos + 1).cloned().unwrap_or_default();
        match parse_moves(&seq) {
            Ok(mv) => {
                let c = apply_moves(&SOLVED, &mv, &tables);
                println!("{}", facelet::to_facelets(&c));
            }
            Err(e) => println!("erro: {e}"),
        }
        return;
    }

    // --- servidor ---
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .or_else(|| {
            args.iter()
                .position(|a| a == "--port")
                .and_then(|i| args.get(i + 1))
                .and_then(|p| p.parse().ok())
        })
        .unwrap_or(8080);

    let state = AppState {
        tables,
        jobs: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        next_job: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let app = Router::new()
        .route("/", get(page_index))
        .route("/style.css", get(page_css))
        .route("/app.js", get(page_js))
        .route("/api/solve", post(api_solve))
        .route("/api/scramble", post(api_scramble))
        .route("/api/apply", post(api_apply))
        .route("/api/optimal/start", post(api_opt_start))
        .route("/api/optimal/status/{id}", get(api_opt_status))
        .route("/api/optimal/cancel/{id}", post(api_opt_cancel))
        .route("/api/health", get(|| async { "ok" }))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("nao consegui abrir a porta {port}: {e}"));
    println!("Servidor no ar:  http://localhost:{port}");
    println!("(Ctrl+C para parar)");
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use coord::*;

    #[test]
    fn coordenadas_ida_e_volta() {
        let mut co = [0u8; 8];
        for i in 0..N_TWIST {
            set_twist(i as u16, &mut co);
            assert_eq!(get_twist(&co) as usize, i);
            assert_eq!(co.iter().map(|&x| x as u32).sum::<u32>() % 3, 0);
        }
        let mut eo = [0u8; 12];
        for i in 0..N_FLIP {
            set_flip(i as u16, &mut eo);
            assert_eq!(get_flip(&eo) as usize, i);
            assert_eq!(eo.iter().map(|&x| x as u32).sum::<u32>() % 2, 0);
        }
        let mut ep = [0u8; 12];
        for i in 0..N_SLICE {
            set_slice(i as u16, &mut ep);
            assert_eq!(get_slice(&ep) as usize, i, "slice {i}");
        }
        let mut p = [0u8; 8];
        for i in 0..N_CPERM {
            perm_from_index(i as u32, 8, &mut p);
            assert_eq!(perm_index(&p) as usize, i);
        }
    }

    #[test]
    fn movimentos_tem_ordem_quatro() {
        let mc = cube::move_cubes();
        for f in 0..6 {
            let m = mc[f * 3];
            let mut c = SOLVED;
            for _ in 0..4 {
                c = c.multiply(&m);
            }
            assert!(c.is_solved(), "face {f} nao voltou ao inicio em 4 giros");
            assert_eq!(mc[f * 3 + 1], SOLVED.multiply(&m).multiply(&m));
        }
    }

    #[test]
    fn planificacao_ida_e_volta() {
        let tables = Tables::build();
        let mut rng = Rng::new();
        for _ in 0..50 {
            let scr = random_scramble(&mut rng, 30);
            let c = apply_moves(&SOLVED, &scr, &tables);
            let f = facelet::to_facelets(&c);
            let back = facelet::to_cubie(&f).expect("planificacao valida");
            assert_eq!(c, back);
        }
        assert_eq!(
            facelet::to_facelets(&SOLVED),
            "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"
        );
    }

    #[test]
    fn estados_invalidos_sao_rejeitados() {
        // canto torcido
        let mut f: Vec<u8> = facelet::to_facelets(&SOLVED).into_bytes();
        f.swap(8, 9);
        f.swap(9, 20);
        let s = String::from_utf8(f).unwrap();
        assert!(facelet::to_cubie(&s).is_err());

        // contagem de cores errada
        assert!(facelet::to_cubie(&"U".repeat(54)).is_err());
        // tamanho errado
        assert!(facelet::to_cubie("UUU").is_err());
    }

    #[test]
    fn rotacao_do_cubo_tem_ordem_tres() {
        let tables = Tables::build();
        let mut rng = Rng::new();
        for axis in 1..3 {
            let pi = &facelet::ROT_PI[axis];
            let rot = facelet::rotation_perm(pi);
            // toda posicao de adesivo tem destino unico
            let mut seen = [false; 54];
            for p in 0..54 {
                assert!(rot[p] < 54 && !seen[rot[p]], "rotacao {axis} nao e bijetiva");
                seen[rot[p]] = true;
            }
            for _ in 0..20 {
                let c = apply_moves(&SOLVED, &random_scramble(&mut rng, 20), &tables);
                let mut r = c;
                for _ in 0..3 {
                    r = facelet::rotate_cube(&r, pi, &rot);
                }
                assert_eq!(c, r, "girar 3x no eixo {axis} deveria voltar ao original");
            }
        }
    }

    fn test_params(timeout_ms: u64) -> search::SolveParams {
        search::SolveParams {
            timeout_ms,
            min_ms: 0,
            ..search::SolveParams::default()
        }
    }

    #[test]
    fn resolve_cubos_aleatorios() {
        let tables = Tables::build();
        let mut rng = Rng::new();
        for i in 0..30 {
            let scr = random_scramble(&mut rng, 25);
            let cube = apply_moves(&SOLVED, &scr, &tables);
            let sol = search::solve(&cube, &tables, test_params(2000))
                .unwrap_or_else(|e| panic!("caso {i}: {e}"));
            let end = apply_moves(&cube, &sol.moves, &tables);
            assert!(end.is_solved(), "caso {i}: solucao nao resolve");
            assert!(
                sol.moves.len() <= 20,
                "caso {i}: {} movimentos (esperado <= 20)",
                sol.moves.len()
            );
        }
    }

    #[test]
    fn superflip_e_resolvido() {
        // Superflip: precisa de exatamente 20 movimentos (distancia maxima conhecida).
        let tables = Tables::build();
        let scr = parse_moves("U R2 F B R B2 R U2 L B2 R U' D' R2 F R' L B2 U2 F2").unwrap();
        let cube = apply_moves(&SOLVED, &scr, &tables);
        let sol = search::solve(&cube, &tables, test_params(5000)).unwrap();
        let end = apply_moves(&cube, &sol.moves, &tables);
        assert!(end.is_solved());
        assert!(sol.moves.len() <= 20, "{} movimentos", sol.moves.len());
    }

    #[test]
    fn cubo_resolvido_da_zero_movimentos() {
        let tables = Tables::build();
        let sol = search::solve(&SOLVED, &tables, test_params(1000)).unwrap();
        assert_eq!(sol.moves.len(), 0);
    }

    #[test]
    fn tabela_twist_flip_e_completa() {
        // Toda combinacao de orientacoes e alcancavel, e a distancia maxima
        // nesse subespaco e pequena (bem abaixo dos 12 da fase 1 completa).
        let tables = Tables::build();
        let max = tables.prun_tf.iter().copied().max().unwrap();
        assert!(tables.prun_tf.iter().all(|&v| v != 255), "estado inalcancavel");
        assert!(max <= 12, "distancia maxima {max} inesperada");
    }

    #[test]
    fn modo_melhor_solucao_encurta() {
        // Com alvo baixo a busca continua depois da primeira solucao;
        // o resultado nunca pode ser pior que o do modo rapido.
        let tables = Tables::build();
        let mut rng = Rng::new();
        for _ in 0..5 {
            let scr = random_scramble(&mut rng, 25);
            let cube = apply_moves(&SOLVED, &scr, &tables);
            let rapido = search::solve(
                &cube,
                &tables,
                search::SolveParams { timeout_ms: 300, min_ms: 0, ..Default::default() },
            )
            .unwrap();
            let melhor = search::solve(
                &cube,
                &tables,
                search::SolveParams {
                    target_len: 15,
                    timeout_ms: 1500,
                    min_ms: 0,
                    ..Default::default()
                },
            )
            .unwrap();
            let end = apply_moves(&cube, &melhor.moves, &tables);
            assert!(end.is_solved());
            assert!(
                melhor.moves.len() <= rapido.moves.len(),
                "melhor ({}) pior que rapido ({})",
                melhor.moves.len(),
                rapido.moves.len()
            );
        }
    }

    fn xform_eq(a: &sym::SymXform, b: &sym::SymXform) -> bool {
        a.perm == b.perm && a.color == b.color
    }

    #[test]
    fn simetrias_formam_grupo_de_16() {
        let syms = sym::symmetries();
        assert_eq!(syms.len(), 16);
        // identidade no indice 0
        assert!(syms[0].perm.iter().enumerate().all(|(i, &p)| i == p));
        // todas distintas
        for i in 0..16 {
            for j in (i + 1)..16 {
                assert!(!xform_eq(&syms[i], &syms[j]), "simetrias {i} e {j} iguais");
            }
        }
        // fechamento: compor duas quaisquer da outra do conjunto
        for a in &syms {
            for b in &syms {
                let mut perm = [0usize; 54];
                let mut color = [0usize; 6];
                for p in 0..54 {
                    perm[p] = b.perm[a.perm[p]];
                }
                for f in 0..6 {
                    color[f] = b.color[a.color[f]];
                }
                let found = syms
                    .iter()
                    .any(|s| s.perm == perm && s.color == color);
                assert!(found, "composicao fora do grupo");
            }
        }
    }

    #[test]
    fn conjugacao_de_movimentos_bate() {
        let syms = sym::symmetries();
        let mc = cube::move_cubes();
        // indices no vetor de simetrias: y = 4 (i=1), f2 = 2 (j=1), espelho = 1 (k=1)
        let y = &syms[4];
        let f2 = &syms[2];
        let m = &syms[1];
        // y: F->L, L->B, B->R, R->F, U->U (mesmo sentido)
        assert_eq!(sym::conj_state(&mc[6], y), mc[12]); // F -> L
        assert_eq!(sym::conj_state(&mc[12], y), mc[15]); // L -> B
        assert_eq!(sym::conj_state(&mc[3], y), mc[6]); // R -> F
        assert_eq!(sym::conj_state(&mc[0], y), mc[0]); // U -> U
        // f2: U->D, R->L, F->F (mesmo sentido)
        assert_eq!(sym::conj_state(&mc[0], f2), mc[9]); // U -> D
        assert_eq!(sym::conj_state(&mc[3], f2), mc[12]); // R -> L
        assert_eq!(sym::conj_state(&mc[6], f2), mc[6]); // F -> F
        // espelho: troca o sentido de tudo; R vira L
        assert_eq!(sym::conj_state(&mc[0], m), mc[2]); // U -> U'
        assert_eq!(sym::conj_state(&mc[3], m), mc[14]); // R -> L'
        assert_eq!(sym::conj_state(&mc[6], m), mc[8]); // F -> F'
    }

    #[test]
    fn conjugacao_de_arestas_bate_com_planificacao() {
        let syms = sym::symmetries();
        let edge_syms: Vec<sym::EdgeCube> = syms.iter().map(sym::edge_cube_of).collect();
        let edge_invs: Vec<sym::EdgeCube> = edge_syms.iter().map(sym::edge_inverse).collect();
        let mut rng = Rng::new();
        for _ in 0..30 {
            // estado so de arestas: permutacao par + orientacoes com soma par
            let mut c = SOLVED;
            for i in (1..12).rev() {
                let j = rng.below(i as u64 + 1) as usize;
                c.ep.swap(i, j);
            }
            if cube::perm_parity(&c.ep) == 1 {
                c.ep.swap(0, 1);
            }
            let mut soma = 0;
            for i in 0..11 {
                c.eo[i] = rng.below(2) as u8;
                soma += c.eo[i];
            }
            c.eo[11] = soma % 2;

            let e = sym::EdgeCube { ep: c.ep, eo: c.eo };
            for s in 0..16 {
                let via_facelets = sym::conj_state(&c, &syms[s]);
                let via_arestas = sym::edge_conj(&e, &edge_syms[s], &edge_invs[s]);
                assert_eq!(via_facelets.ep, via_arestas.ep, "ep difere na simetria {s}");
                assert_eq!(via_facelets.eo, via_arestas.eo, "eo difere na simetria {s}");
            }
        }
    }

    #[test]
    fn coordenada_flip_slice_nao_depende_do_preenchimento() {
        // O par (slice, flip) conjugado nao pode depender de QUAIS arestas estao
        // em quais posicoes da fatia, so de ONDE a fatia esta e das orientacoes.
        let syms = sym::symmetries();
        let edge_syms: Vec<sym::EdgeCube> = syms.iter().map(sym::edge_cube_of).collect();
        let edge_invs: Vec<sym::EdgeCube> = edge_syms.iter().map(sym::edge_inverse).collect();
        let mut rng = Rng::new();
        for _ in 0..200 {
            let raw = rng.below(sym::N_RAW as u64) as usize;
            let c = sym::edges_of_raw(raw);
            // relabel aleatorio das arestas dentro de cada grupo (fatia / resto)
            let mut rho: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
            for i in (1..8).rev() {
                let j = rng.below(i as u64 + 1) as usize;
                rho.swap(i, j);
            }
            for i in (9..12).rev() {
                let j = 8 + rng.below((i - 8) as u64 + 1) as usize;
                rho.swap(i, j);
            }
            let mut c2 = c;
            for i in 0..12 {
                c2.ep[i] = rho[c.ep[i] as usize];
            }
            assert_eq!(sym::raw_of_edges(&c2), raw, "relabel mudou a coordenada");
            for s in 0..16 {
                let a = sym::raw_of_edges(&sym::edge_conj(&c, &edge_syms[s], &edge_invs[s]));
                let b = sym::raw_of_edges(&sym::edge_conj(&c2, &edge_syms[s], &edge_invs[s]));
                assert_eq!(a, b, "conjugado depende do preenchimento (sim {s}, raw {raw})");
            }
        }
    }

    #[test]
    fn mapeamento_de_eixos_esta_certo() {
        // Acompanhar as coordenadas dos 3 eixos com movimentos mapeados tem que
        // dar o mesmo resultado que girar o cubo inteiro e ler as coordenadas.
        let tables = Tables::build();
        let am = optimal::axis_move_table();
        let mut rng = Rng::new();
        for _ in 0..20 {
            let scr = random_scramble(&mut rng, 15);
            let state = apply_moves(&SOLVED, &scr, &tables);
            let want = optimal::coords_of(&state);
            let mut got = [(0u16, 0u16, 0u16); 3];
            for a in 0..3 {
                let (mut t, mut f, mut s) = (0u16, 0u16, 0u16);
                for &m in &scr {
                    let ma = am[a][m as usize] as usize;
                    t = tables.twist_move[t as usize * 18 + ma];
                    f = tables.flip_move[f as usize * 18 + ma];
                    s = tables.slice_move[s as usize * 18 + ma];
                }
                got[a] = (t, f, s);
            }
            assert_eq!(want, got);
        }
    }

    #[test]
    fn modo_otimo_prova_e_resolve() {
        let mut tables = Tables::build();
        tables.big = Some(sym::BigP1::load_or_build(&tables, None, false));
        let mut rng = Rng::new();

        // sem a tabela grande o modo otimo recusa
        let sem_big = Tables::build();
        let scr = random_scramble(&mut rng, 10);
        let cube = apply_moves(&SOLVED, &scr, &sem_big);
        assert!(optimal::solve_optimal(&cube, &sem_big, 1000, 4, None).is_err());

        // cubos faceis: prova rapida e tamanho nunca maior que o embaralhamento
        for len in [4usize, 6, 8] {
            let scr = random_scramble(&mut rng, len);
            let cube = apply_moves(&SOLVED, &scr, &tables);
            let o = optimal::solve_optimal(&cube, &tables, 20_000, search::default_threads(), None)
                .unwrap();
            assert!(apply_moves(&cube, &o.moves, &tables).is_solved());
            assert!(o.optimal, "cubo de {len} movimentos deveria ser provado otimo");
            assert!(o.moves.len() <= len);
            assert_eq!(o.lower_bound, o.moves.len());
        }

        // cubo dificil com pouco tempo: solucao valida + limite provado coerente
        let scr = random_scramble(&mut rng, 25);
        let cube = apply_moves(&SOLVED, &scr, &tables);
        let o = optimal::solve_optimal(&cube, &tables, 3_000, search::default_threads(), None).unwrap();
        assert!(apply_moves(&cube, &o.moves, &tables).is_solved());
        assert!(o.lower_bound <= o.moves.len());
        assert!(o.lower_bound >= 8, "limite inferior {} suspeito", o.lower_bound);
        if o.optimal {
            assert_eq!(o.lower_bound, o.moves.len());
        }
    }

    #[test]
    fn epos_ida_e_volta_e_move() {
        // ida e volta da coordenada
        let mut ep = [0u8; 12];
        for i in 0..N_EPOS {
            set_epos(i as u16, &mut ep);
            assert_eq!(get_epos(&ep) as usize, i, "epos {i}");
        }
        // aplicar so a permutacao de arestas do movimento (como a tabela
        // epos_move faz) da o mesmo epos que a multiplicacao completa
        let tables = Tables::build();
        let mut rng = Rng::new();
        for _ in 0..100 {
            let scr = random_scramble(&mut rng, 15);
            let c = apply_moves(&SOLVED, &scr, &tables);
            for m in 0..18 {
                let d = c.multiply(&tables.mc[m]);
                let mut ep2 = [0u8; 12];
                for i in 0..12 {
                    ep2[i] = c.ep[tables.mc[m].ep[i] as usize];
                }
                assert_eq!(get_epos(&d.ep), get_epos(&ep2));
            }
        }
        // epos refina slice: mesmo conjunto de posicoes
        for _ in 0..50 {
            let scr = random_scramble(&mut rng, 20);
            let c = apply_moves(&SOLVED, &scr, &tables);
            let mut ep2 = [0u8; 12];
            set_epos(get_epos(&c.ep), &mut ep2);
            assert_eq!(get_slice(&ep2), get_slice(&c.ep));
        }
    }

    /// Pesado (~2-4 min): constroi a tabela X inteira e valida contra a fase 1.
    /// Rodar com: cargo test --release tabela_x -- --ignored --nocapture
    #[test]
    #[ignore]
    fn tabela_x_e_exata_e_domina() {
        let mut tables = Tables::build();
        tables.big = Some(sym::BigP1::load_or_build(&tables, None, false));
        let x = xtable::BigX::load_or_build(&tables, None, true);
        let big = tables.big.as_ref().unwrap();

        // estado resolvido e o objetivo
        assert!(x.is_goal(0, 0, coord::epos_solved()));
        assert_eq!(x.exact(&tables, 0, 0, coord::epos_solved()), 0);

        let mut rng = Rng::new();
        // exatidao incremental: seguir um caminho aleatorio acompanhando o
        // mod 3 tem que terminar na mesma distancia que a descida da tabela
        for _ in 0..50 {
            let scr = random_scramble(&mut rng, 25);
            let c = apply_moves(&SOLVED, &scr, &tables);
            let (mut tw, mut f, mut e) =
                (get_twist(&c.co), get_flip(&c.eo), get_epos(&c.ep));
            let mut exact = x.exact(&tables, tw, f, e) as i32;
            let mut m3 = x.m3(tw, f, e);
            for _ in 0..30 {
                let m = rng.below(18) as usize;
                tw = tables.twist_move[tw as usize * 18 + m];
                f = tables.flip_move[f as usize * 18 + m];
                e = x.epos_move[e as usize * 18 + m];
                let m3n = x.m3(tw, f, e);
                exact += match (m3n + 3 - m3) % 3 {
                    1 => 1,
                    2 => -1,
                    _ => 0,
                };
                m3 = m3n;
            }
            assert_eq!(exact, x.exact(&tables, tw, f, e) as i32, "rastreamento divergiu");
        }

        // dominancia: distancia X >= distancia exata de fase 1 (e o refinamento)
        for _ in 0..300 {
            let scr = random_scramble(&mut rng, 22);
            let c = apply_moves(&SOLVED, &scr, &tables);
            let hx = x.exact(&tables, get_twist(&c.co), get_flip(&c.eo), get_epos(&c.ep));
            let h1 = big.h(get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep));
            assert!(hx >= h1, "X ({hx}) menor que fase 1 ({h1})");
        }

        // admissibilidade: estado a k movimentos tem distancia <= k
        for _ in 0..100 {
            let k = 1 + rng.below(10) as usize;
            let scr = random_scramble(&mut rng, k);
            let c = apply_moves(&SOLVED, &scr, &tables);
            let hx = x.exact(&tables, get_twist(&c.co), get_flip(&c.eo), get_epos(&c.ep));
            assert!(hx as usize <= k, "hx = {hx} para estado a {k} movimentos");
        }

        // o prover com a tabela X da os MESMOS otimos que o prover da fase 1
        tables.bigx = Some(x);
        let mut casos = Vec::new();
        for _ in 0..3 {
            let scr = random_scramble(&mut rng, 12);
            casos.push(apply_moves(&SOLVED, &scr, &tables));
        }
        for (i, cube) in casos.iter().enumerate() {
            let com_x =
                optimal::solve_optimal(cube, &tables, 60_000, search::default_threads(), None)
                    .unwrap();
            assert!(com_x.optimal, "caso {i} nao provado com X");
            assert!(apply_moves(cube, &com_x.moves, &tables).is_solved());
            let sem_x = {
                let bx = tables.bigx.take();
                let r =
                    optimal::solve_optimal(cube, &tables, 60_000, search::default_threads(), None)
                        .unwrap();
                tables.bigx = bx;
                r
            };
            assert!(sem_x.optimal, "caso {i} nao provado sem X");
            assert_eq!(
                com_x.moves.len(),
                sem_x.moves.len(),
                "caso {i}: otimos diferentes ({} com X, {} sem)",
                com_x.moves.len(),
                sem_x.moves.len()
            );
        }
    }

    #[test]
    fn conjugacao_de_cantos_bate_com_planificacao() {
        let syms = sym::symmetries();
        let cs: Vec<[u8; 8]> = syms.iter().map(sym::corner_perm_of).collect();
        let ci: Vec<[u8; 8]> = cs.iter().map(sym::perm8_inverse).collect();
        let mut rng = Rng::new();
        for _ in 0..30 {
            // permutacao PAR de cantos (arestas na identidade mantem a paridade)
            let mut c = SOLVED;
            for i in (1..8).rev() {
                let j = rng.below(i as u64 + 1) as usize;
                c.cp.swap(i, j);
            }
            if cube::perm_parity(&c.cp) == 1 {
                c.cp.swap(0, 1);
            }
            for s in 0..16 {
                let via_facelets = sym::conj_state(&c, &syms[s]);
                let via_indices = sym::cperm_conj(&c.cp, &cs[s], &ci[s]);
                assert_eq!(via_facelets.cp, via_indices, "cperm difere na simetria {s}");
            }
        }
    }

    #[test]
    fn tabela_p2_e_consistente() {
        let mut tables = Tables::build();
        let big2 = sym::BigP2::load_or_build(&tables, None, false);

        // completa (paridade nao restringe o par cperm x uperm) e maximo conhecido
        assert!(big2.dist.iter().all(|&v| v != 255), "estado inalcancavel");
        let max = big2.dist.iter().copied().max().unwrap();
        assert!(max <= 18, "distancia maxima {max} (esperado <= 18)");

        // identidade = 0; qualquer movimento de G1 mexe em cantos ou arestas U/D
        assert_eq!(big2.h2(0, 0), 0);
        for &m in &cube::P2_MOVES {
            let c = SOLVED.multiply(&tables.mc[m as usize]);
            assert_eq!(big2.h2(get_cperm(&c.cp), get_uperm(&c.ep)), 1, "movimento {m}");
        }

        let mut rng = Rng::new();
        for _ in 0..200 {
            // estado de G1 com k movimentos: h2 admissivel (<= k) e consistente
            let k = 1 + rng.below(14) as usize;
            let mut c = SOLVED;
            for _ in 0..k {
                let m = cube::P2_MOVES[rng.below(10) as usize];
                c = c.multiply(&tables.mc[m as usize]);
            }
            let h = big2.h2(get_cperm(&c.cp), get_uperm(&c.ep));
            assert!(h as usize <= k, "h2 = {h} para estado a {k} movimentos");
            for &m in &cube::P2_MOVES {
                let d = c.multiply(&tables.mc[m as usize]);
                let h2 = big2.h2(get_cperm(&d.cp), get_uperm(&d.ep));
                assert!((h as i32 - h2 as i32).abs() <= 1, "vizinhos {h} e {h2}");
            }
        }

        // e o solver continua correto com a tabela ligada
        tables.big2 = Some(big2);
        for i in 0..8 {
            let scr = random_scramble(&mut rng, 25);
            let cube = apply_moves(&SOLVED, &scr, &tables);
            let sol = search::solve(&cube, &tables, test_params(2000))
                .unwrap_or_else(|e| panic!("caso {i}: {e}"));
            assert!(apply_moves(&cube, &sol.moves, &tables).is_solved());
            assert!(sol.moves.len() <= 20);
        }
    }

    #[test]
    fn tabela_grande_e_exata_e_resolve() {
        let mut tables = Tables::build();
        let big = sym::BigP1::load_or_build(&tables, None, false);

        // completa e com o maximo conhecido da fase 1
        assert!(big.dist.iter().all(|&v| v != 255), "estado inalcancavel");
        let max = big.dist.iter().copied().max().unwrap();
        assert!(max <= 12, "distancia maxima {max} (esperado <= 12)");

        // resolvido = 0; movimentos de G1 continuam a distancia 0,
        // os quartos de volta de R/F/L/B ficam a distancia 1
        assert_eq!(big.h(0, 0, 0), 0);
        for m in 0..18u8 {
            let c = SOLVED.multiply(&tables.mc[m as usize]);
            let h = big.h(get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep));
            let esperado = if cube::P2_MOVES.contains(&m) { 0 } else { 1 };
            assert_eq!(h, esperado, "movimento {m} com distancia errada");
        }

        // estados dentro de G1 tem distancia 0
        let mut rng = Rng::new();
        for _ in 0..20 {
            let mut c = SOLVED;
            for _ in 0..15 {
                let m = cube::P2_MOVES[rng.below(10) as usize];
                c = c.multiply(&tables.mc[m as usize]);
            }
            assert_eq!(big.h(get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep)), 0);
        }

        // consistencia (vizinhos diferem em <= 1) e dominancia sobre a heuristica antiga
        for _ in 0..200 {
            let scr = random_scramble(&mut rng, 20);
            let c = apply_moves(&SOLVED, &scr, &tables);
            let h = big.h(get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep));
            let old = tables.prun1(get_twist(&c.co), get_flip(&c.eo), get_slice(&c.ep));
            assert!(h >= old, "exata ({h}) menor que a heuristica antiga ({old})");
            for m in 0..18 {
                let d = c.multiply(&tables.mc[m]);
                let h2 = big.h(get_twist(&d.co), get_flip(&d.eo), get_slice(&d.ep));
                assert!(
                    (h as i32 - h2 as i32).abs() <= 1,
                    "vizinhos com distancias {h} e {h2}"
                );
            }
        }

        // e resolve cubos com a tabela ligada
        tables.big = Some(big);
        for i in 0..8 {
            let scr = random_scramble(&mut rng, 25);
            let cube = apply_moves(&SOLVED, &scr, &tables);
            let sol = search::solve(&cube, &tables, test_params(2000))
                .unwrap_or_else(|e| panic!("caso {i}: {e}"));
            assert!(apply_moves(&cube, &sol.moves, &tables).is_solved());
            assert!(sol.moves.len() <= 20);
        }
    }

    #[test]
    fn max_len_e_respeitado() {
        let tables = Tables::build();
        let mut rng = Rng::new();
        let scr = random_scramble(&mut rng, 25);
        let cube = apply_moves(&SOLVED, &scr, &tables);
        let sol = search::solve(
            &cube,
            &tables,
            search::SolveParams {
                max_len: 22,
                target_len: 22,
                timeout_ms: 500,
                min_ms: 0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(sol.moves.len() <= 22);
        assert!(apply_moves(&cube, &sol.moves, &tables).is_solved());
    }
}

