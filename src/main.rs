mod coord;
mod cube;
mod facelet;
mod search;
mod tables;

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
    #[serde(default)]
    max_len: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
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
    threads: usize,
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

async fn api_solve(
    State(t): State<Arc<Tables>>,
    Json(req): Json<SolveReq>,
) -> Result<Json<SolveResp>, ApiError> {
    let cube = facelet::to_cubie(&req.facelets).map_err(bad_request)?;
    let max_len = req.max_len.unwrap_or(20).clamp(1, 30);
    let timeout_ms = req.timeout_ms.unwrap_or(4000).clamp(50, 30_000);

    let res = tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        let sol = search::solve(&cube, &t, max_len, timeout_ms, search::default_threads());
        sol.map(|s| {
            let elapsed = start.elapsed().as_millis();
            let mut states = Vec::with_capacity(s.moves.len() + 1);
            let mut c = cube;
            states.push(facelet::to_facelets(&c));
            for &m in &s.moves {
                c = c.multiply(&t.mc[m as usize]);
                states.push(facelet::to_facelets(&c));
            }
            let names = notation(&s.moves);
            SolveResp {
                notation: names.join(" "),
                length: s.moves.len(),
                phase1: s.phase1,
                phase2: s.moves.len() - s.phase1,
                time_ms: elapsed,
                nodes: s.nodes,
                threads: s.threads,
                solution: names,
                states,
            }
        })
    })
    .await
    .map_err(|e| bad_request(format!("falha interna: {e}")))?;

    res.map(Json).map_err(bad_request)
}

async fn api_scramble(
    State(t): State<Arc<Tables>>,
    Json(req): Json<ScrambleReq>,
) -> Json<ScrambleResp> {
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
    State(t): State<Arc<Tables>>,
    Json(req): Json<ApplyReq>,
) -> Result<Json<ApplyResp>, ApiError> {
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

fn bench_timeout() -> u64 {
    std::env::var("BENCH_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(4000)
}

fn run_bench(t: &Tables, n: usize) {
    let mut rng = Rng::new();
    let mut total_len = 0usize;
    let mut total_ms = 0u128;
    let mut worst = 0usize;
    let mut worst_ms = 0u128;
    let mut hist = [0usize; 31];
    println!("Resolvendo {n} cubos aleatorios com {} threads...", search::default_threads());
    for i in 0..n {
        let scr = random_scramble(&mut rng, 25);
        let cube = apply_moves(&SOLVED, &scr, t);
        let start = Instant::now();
        let sol = search::solve(&cube, t, 20, bench_timeout(), search::default_threads())
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
    print!("distribuicao:");
    for (l, &c) in hist.iter().enumerate() {
        if c > 0 {
            print!(" {l}:{c}");
        }
    }
    println!();
}

// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    print!("Gerando tabelas... ");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let t0 = Instant::now();
    let tables = Arc::new(Tables::build());
    println!("pronto em {} ms", t0.elapsed().as_millis());

    // --- modos CLI ---
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let n = args
            .get(pos + 1)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(100);
        run_bench(&tables, n);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--solve") {
        let input = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("");
        match facelet::to_cubie(input) {
            Ok(c) => match search::solve(&c, &tables, 20, 1500, search::default_threads()) {
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

    let app = Router::new()
        .route("/", get(page_index))
        .route("/style.css", get(page_css))
        .route("/app.js", get(page_js))
        .route("/api/solve", post(api_solve))
        .route("/api/scramble", post(api_scramble))
        .route("/api/apply", post(api_apply))
        .route("/api/health", get(|| async { "ok" }))
        .with_state(tables);

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

    #[test]
    fn resolve_cubos_aleatorios() {
        let tables = Tables::build();
        let mut rng = Rng::new();
        for i in 0..30 {
            let scr = random_scramble(&mut rng, 25);
            let cube = apply_moves(&SOLVED, &scr, &tables);
            let sol = search::solve(&cube, &tables, 20, 2000, search::default_threads())
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
        let sol = search::solve(&cube, &tables, 20, 5000, search::default_threads()).unwrap();
        let end = apply_moves(&cube, &sol.moves, &tables);
        assert!(end.is_solved());
        assert!(sol.moves.len() <= 20, "{} movimentos", sol.moves.len());
    }

    #[test]
    fn cubo_resolvido_da_zero_movimentos() {
        let tables = Tables::build();
        let sol = search::solve(&SOLVED, &tables, 20, 1000, 4).unwrap();
        assert_eq!(sol.moves.len(), 0);
    }
}

