//! Cubos grandes genericos (5x5, 6x6 e 7x7), resolvidos por REDUCAO.
//!
//! A observacao que torna isso tratavel: em qualquer NxN, cada ORBITA de
//! centros (posicoes equivalentes sob a rotacao da face) e cada ORBITA de
//! asas (pares de pecas de aresta a mesma distancia do meio) e isomorfa as do
//! 4x4 — 24 pecas, 4 (ou 2) por face/aresta. Entao a arquitetura provada do
//! 4x4 se aplica orbita a orbita: tabelas exatas por face (C(24,4)), tabelas
//! de par com bit de flip relativo, busca gulosa + dirigida com raiz
//! paralela.
//!
//! Pecas por tamanho:
//!   - 8 cantos (como no 3x3);
//!   - impares (5, 7): 12 "midges" (a peca central de cada aresta) + centros
//!     fixos no meio de cada face (definem o esquema de cores);
//!   - orbitas de asas: 1 no 5x5, 2 no 6x6 e no 7x7;
//!   - orbitas de centros: 2 no 5x5, 4 no 6x6, 6 no 7x7.
//!
//! PARIDADES sem algoritmos decorados: se o mapa 3x3 da reducao sair invalido
//! (orientacao ou permutacao), insere-se um quarto de fatia interna — que
//! alterna a paridade — e as etapas afetadas (centros das orbitas tocadas +
//! reagrupamento) sao REFEITAS. Cada tentativa e revalidada; bounded e
//! auto-verificavel, sem depender de sequencias de memoria.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::search::{self, SolveParams};
use crate::tables::Tables;

// ---------------------------------------------------------------------------
// Geometria basica
// ---------------------------------------------------------------------------

const FACES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

#[inline]
fn p(n: usize, face: usize, r: usize, c: usize) -> usize {
    face * n * n + r * n + c
}

/// Configuracao completa de um tamanho N (gerada uma vez e cacheada).
pub struct CubeN {
    pub n: usize,
    pub n_facelets: usize,
    /// permutacoes dos movimentos: 6 faces x depth (1..=n/2) x 3 potencias
    pub moves: Vec<Vec<u16>>,
    pub n_moves: usize,
    depths: usize,
    /// cantos: 3 adesivos cada, ordem de cores igual ao 3x3
    corner_facelets: [[usize; 3]; 8],
    /// midges (so impar): 2 adesivos cada, ordem igual as arestas do 3x3
    midge_facelets: Option<[[usize; 2]; 12]>,
    /// orbitas de asas: cada uma com 24 pares (2 por encaixe de aresta)
    wing_orbits: Vec<[[usize; 2]; 24]>,
    /// orbitas de centros: cada uma com 24 encaixes (4 por face, em ordem)
    center_orbits: Vec<[usize; 24]>,
    // ---- tabelas por orbita (mesma estrutura do 4x4) ----
    /// [orbita][movimento][slot] -> destino (centros)
    cmove: Vec<Vec<Vec<u8>>>,
    /// [orbita][movimento][asa] -> destino
    wmove: Vec<Vec<Vec<u8>>>,
    /// [orbita][movimento] bit q -> chegada com ordem trocada
    wflip: Vec<Vec<u32>>,
    /// midges: [movimento][slot] -> (destino, flip)
    mmove: Vec<[(u8, u8); 12]>,
    /// [orbita][face][rank C(24,4)] -> distancia exata
    center_dist: Vec<[Vec<u8>; 6]>,
    /// [orbita][face][pos] -> minimo de movimentos para uma peca em pos chegar na face
    #[allow(dead_code)] // util para heuristicas de centro; hoje a subida global basta
    center_pos_dist: Vec<[[u8; 24]; 6]>,
    /// [movimento][face de origem] -> face de destino do centro do meio (impar)
    midmove: Vec<[u8; 6]>,
    /// [orbita][(a*24+b)*2+rel] -> distancia exata do par
    pair_dist: Vec<Vec<u8>>,
    /// [orbita] tabela conjunta de DOIS pares (fim de jogo), como no 4x4
    pair2_dist: Vec<Vec<u8>>,
    /// [orbita] comutador que e 3-ciclo PURO dessa orbita de centro
    base3: Vec<Option<Vec<usize>>>,
    /// [orbita] as variantes gratis do ciclo base (rotacoes e inverso), cada
    /// uma com seu trio-suporte: sao as raizes da arvore multi-fonte
    base3_fontes: Vec<Vec<([u8; 3], Vec<usize>)>>,
    /// [orbita] arvore multi-raiz: (pai, movimento, fonte) por trio ordenado
    triple_trees: Vec<Vec<(u16, u8, u8)>>,
    /// idem para as orbitas de ASAS
    wbase3: Vec<Option<Vec<usize>>>,
    wbase3_fontes: Vec<Vec<([u8; 3], Vec<usize>)>>,
    wing_trees: Vec<Vec<(u16, u8, u8)>>,
    /// Cubos pares: sequencia que troca a paridade dos pares invertidos da
    /// orbita 0 SEM mexer nos centros. Certificada por simulacao no build.
    flip_alg: Option<Vec<usize>>,
}

static REGISTRY: OnceLock<Mutex<HashMap<usize, Arc<CubeN>>>> = OnceLock::new();

pub fn cuben(n: usize) -> Arc<CubeN> {
    let reg = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let g = reg.lock().unwrap();
        if let Some(c) = g.get(&n) {
            return c.clone();
        }
    }
    let built = Arc::new(CubeN::build(n));
    built.verify_compact();
    reg.lock().unwrap().insert(n, built.clone());
    built
}

// ---------------------------------------------------------------------------
// Pipeline de reducao com retry de paridade
// ---------------------------------------------------------------------------

pub struct StageN {
    pub name: String,
    pub info: String,
    pub tokens: Vec<String>,
}

pub struct SolveN {
    pub stages: Vec<StageN>,
    pub states: Vec<String>,
    pub length: usize,
}

const EDGE_NAMES_N: [&str; 12] = [
    "cima-direita", "cima-frente", "cima-esquerda", "cima-trás",
    "baixo-direita", "baixo-frente", "baixo-esquerda", "baixo-trás",
    "frente-direita", "frente-esquerda", "trás-esquerda", "trás-direita",
];

/// Relato de progresso: etapa atual e quantos movimentos ja sairam.
pub type ProgFn<'a> = &'a (dyn Fn(&str, usize) + Send + Sync);

/// 0 = silencioso, 1 = etapas lentas, 2 = tudo (com estados, para replicar).
pub fn debug_level() -> u8 {
    std::env::var("CUBEN_DEBUG").ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Threads das buscas paralelas: usa TODOS os processadores logicos (numa
/// maquina de 12 nucleos / 24 threads, 24 — havia um teto de 12 que deixava
/// metade parada). Sob `cargo test` os testes ja rodam em paralelo entre si,
/// entao cada busca usa uma fracao para nao disputarem os mesmos nucleos:
/// medido, 12 threads por busca faziam 34s isolados virarem 392s na suite.
///
/// `CUBEN_WORKERS=1` deixa a busca DETERMINISTICA, e isso importa para medir:
/// as buscas paralelas param no primeiro achado, entao quem vence depende de
/// qual thread chegou antes. MEDIDO: o mesmo binario, com a mesma semente,
/// resolveu um 6x6 em 926 movimentos/3.3s numa rodada e 956/10.3s na seguinte.
/// Comparar duas versoes sem fixar isso e comparar ruido.
fn n_workers() -> usize {
    if let Some(w) = std::env::var("CUBEN_WORKERS").ok().and_then(|v| v.parse::<usize>().ok()) {
        return w.max(1);
    }
    let total = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(4);
    if cfg!(test) {
        return (total / 4).clamp(2, 8);
    }
    total.clamp(1, 64)
}

#[cfg(test)]
pub fn solve_n(n: usize, input: &str, t: &Tables) -> Result<SolveN, String> {
    solve_n_prog(n, input, t, None)
}

pub fn solve_n_prog(
    n: usize,
    input: &str,
    t: &Tables,
    prog: Option<ProgFn>,
) -> Result<SolveN, String> {
    let say = |msg: &str, len: usize| {
        if let Some(p) = prog {
            p(msg, len);
        }
    };
    let cn = cuben(n);
    let dbg = debug_level();
    let (mut state, letters) = cn.parse(input)?;
    if dbg >= 2 {
        eprintln!("[solve_n] N={n} entrada: {}", cn.render(&state, &letters));
    }
    let mut stages: Vec<StageN> = Vec::new();
    let mut states = vec![cn.render(&state, &letters)];

    let push_stage = |cn: &CubeN,
                      state: &mut Vec<u8>,
                      states: &mut Vec<String>,
                      stages: &mut Vec<StageN>,
                      name: String,
                      info: String,
                      seq: &[usize]| {
        if seq.is_empty() {
            return;
        }
        let mut tokens = Vec::new();
        for &m in seq {
            cn.apply(state, m);
            states.push(cn.render(state, &letters));
            tokens.push(cn.move_name(m));
        }
        // etapas consecutivas de mesmo nome viram uma so
        if let Some(last) = stages.last_mut() {
            if last.name == name {
                last.tokens.extend(tokens);
                return;
            }
        }
        stages.push(StageN { name, info, tokens });
    };

    let n_corb = cn.center_orbits.len();
    let n_worb = cn.wing_orbits.len();

    // refazer centros+asas custa segundos, entao vale insistir bastante.
    // Cada tentativa recomeca do estado ORIGINAL com as perturbacoes de
    // paridade como prefixo: o trabalho perdido nao entra na solucao final.
    //
    // TENTADO E REVERTIDO, tres vezes, sempre pior (medido com CUBEN_WORKERS=1,
    // nos mesmos 6 cubos): retomar do ponto "centros prontos" em vez de refazer.
    // Parece obvio — os centros ja estao la, e a correcao so os perturba um
    // pouco —, mas
    //   1. reparar centros perturbados NAO e mais barato que monta-los do zero:
    //      a subida monotona lida bem com um cubo generico e mal com um quase
    //      pronto que levou um giro largo (218s -> 341s so nos 5 primeiros
    //      casos);
    //   2. o reparo entra na solucao. Marcando o ponto a cada tentativa, o
    //      reparo de uma sobrevive na seguinte e a coisa incha (5480 -> 9248
    //      movimentos); com marca unica ainda sobra o reparo da ultima.
    // Descartar trabalho sai mais barato que reaproveita-lo.
    let max_attempts = 24;
    let inicial = state.clone();
    let mut prefixo: Vec<usize> = Vec::new();
    'attempt: for attempt in 0..max_attempts {
        {
        state = inicial.clone();
        stages.clear();
        states = vec![cn.render(&state, &letters)];
        if !prefixo.is_empty() {
            let p = prefixo.clone();
            push_stage(
                &cn,
                &mut state,
                &mut states,
                &mut stages,
                "Ajuste de paridade".into(),
                "Troca a paridade antes de reduzir; sem isso o cubo não fecha.".into(),
                &p,
            );
        }
        }
        // ---- centros: subida monotonica da medida global -----------------
        // Cada passo aumenta o numero de centros na face certa (contando os
        // centros do meio nos impares). Nada e "protegido": basta que o total
        // suba, o que torna impossivel o ciclo constroi-destroi.
        {
            let alvo_total = cn.center_total_max();
            let mut guard = 0;
            let mut kicks = 0usize;
            let mut vistos: Vec<u64> = Vec::new();
            // Se o melhor total nao melhora por muitos passos, a fase esta
            // oscilando (medido: 142<->146 no 7x7, 8s por passo). Em vez de
            // insistir por horas, reinicia com perturbacao — mesma saida usada
            // para paridade, e com tempo limitado.
            let mut melhor = 0usize;
            let mut sem_avanco = 0usize;
            let mut travou_centros = false;
            loop {
                let cs = cn.cstate_of(&state);
                let total = cn.center_total(&cs);
                if total > melhor {
                    melhor = total;
                    sem_avanco = 0;
                } else {
                    sem_avanco += 1;
                    if sem_avanco > 80 {
                        travou_centros = true;
                        break;
                    }
                }
                vistos.push(cn.center_sig(&cs));
                if vistos.len() > 60 {
                    vistos.remove(0);
                }
                if total == alvo_total {
                    break;
                }
                guard += 1;
                if guard > 1500 {
                    if dbg >= 1 {
                        eprintln!(
                            "[centros] TRAVOU em {total}/{alvo_total}\n  estado: {}\n  \
                             replicar: CUBEN_STATE=<estado> cargo test replicar_centros",
                            cn.render(&state, &letters)
                        );
                    }
                    return Err(format!("centros nao fecharam ({total}/{alvo_total})"));
                }
                say(
                    &format!("Centros ({total}/{alvo_total})"),
                    stages.iter().map(|s| s.tokens.len()).sum(),
                );
                let t0 = std::time::Instant::now();
                // Tentei elevar o esforco da busca construtiva quando a fase
                // empaca (teto de 120 mil em vez de 4 mil), esperando evitar o
                // reinicio. MEDIDO na regua: o caso ruim do 7x7 continuou em
                // ~10 min e os casos normais pioraram 30 a 50% (6x6 de 37s para
                // 53s). Nao compensa — o teto fixo fica.
                match cn.improve_centers(&cs, total) {
                    Some(seq) => {
                        let gasto = t0.elapsed().as_secs_f64();
                        if dbg >= 1 {
                            // casa com a linha CDEGRAU do degrau vencedor: quem
                            // produz os MOVIMENTOS dos centros, e a que preco
                            let mut s2 = cs;
                            for &m in &seq {
                                s2 = cn.capply(&s2, m);
                            }
                            eprintln!(
                                "CGASTO mov={} pecas=+{}",
                                seq.len(),
                                cn.center_total(&s2).saturating_sub(total)
                            );
                        }
                        if dbg >= 2 {
                            eprintln!(
                                "[centros #{guard}] {total}->? via {} ({:.2}s)\n  estado: {}",
                                seq.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" "),
                                gasto,
                                cn.render(&state, &letters)
                            );
                        } else if dbg >= 1 && gasto > 2.0 {
                            eprintln!(
                                "centros {total}/{alvo_total}: {} mov em {gasto:.2}s",
                                seq.len()
                            );
                        }
                        push_stage(
                            &cn,
                            &mut state,
                            &mut states,
                            &mut stages,
                            "Montar os centros".into(),
                            "Agrupe cada cor de centro na sua face.".into(),
                            &seq,
                        );
                    }
                    None => {
                        // platô: rearranja sem perder terreno e tenta de novo
                        let k = kicks;
                        kicks += 1;
                        let ch = cn.plateau_shuffle(&cs, total, k, &vistos);
                        if dbg >= 2 {
                            eprintln!(
                                "[centros #{guard}] platô {k} em {total}/{alvo_total} via {}",
                                ch.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" ")
                            );
                        } else if dbg >= 1 && k % 50 == 0 {
                            eprintln!("centros {total}/{alvo_total}: platô #{k}");
                        }
                        push_stage(
                            &cn,
                            &mut state,
                            &mut states,
                            &mut stages,
                            "Reposicionar centros".into(),
                            "Rearranja os centros que faltam sem perder os que já estão certos."
                                .into(),
                            &ch,
                        );
                    }
                }
            }
            if travou_centros {
                if attempt + 1 == max_attempts {
                    return Err(format!("centros oscilando (melhor {melhor}/{alvo_total})"));
                }
                if dbg >= 1 {
                    eprintln!("centros oscilando em {melhor}/{alvo_total}: reinicia perturbado");
                }
                let mut r = 0x51_7c_c1_b7_27_22_0a_95u64
                    ^ (attempt as u64 + 3) * 0x2545_f491_4f6c_dd1d;
                for _ in 0..(3 + attempt % 4) {
                    r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    let m = ((r >> 33) % cn.n_moves as u64) as usize;
                    if prefixo.last().is_some_and(|&l| l / 3 == m / 3) {
                        continue;
                    }
                    prefixo.push(m);
                }
                continue 'attempt;
            }
        }
        {
            let cs = cn.cstate_of(&state);
            if !cn.c_centers_solved(&cs, n_corb, &[]) {
                return Err("erro interno: centros nao fecharam".into());
            }
        }

        // ---- asas, orbita a orbita --------------------------------------
        if dbg >= 1 {
            let cs = cn.cstate_of(&state);
            let sinais: Vec<String> = (0..n_worb)
                .map(|oi| format!("o{oi}={}", cn.wing_state_sign_odd(&cs, oi) as u8))
                .collect();
            eprintln!("SINAL apos centros (tentativa {attempt}): {}", sinais.join(" "));
        }
        let all_bs = cn.wing_bs(false);
        let mut wing_deadlock: Option<usize> = None;
        // Rodadas de agrupamento: se ao fim a paridade dos pares invertidos
        // estiver impar (o 3x3 recusaria), aplicamos a sequencia certificada
        // que troca essa paridade sem mexer nos centros e reagrupamos. Os
        // centros ficam prontos, entao a segunda rodada e barata.
        let mut rodadas = 0;
        // instrumentacao: as duas correcoes de paridade (a de dentro do
        // agrupamento e a do fim da rodada) usam a MESMA sequencia; se elas se
        // desfizerem, o retrato abaixo mostra a oscilacao.
        let diag = |cn: &CubeN, cs: &SN| -> String {
            let inv: Vec<usize> = (0..n_worb).map(|o| cn.invertidos(cs, o)).collect();
            let grp: Vec<usize> = (0..n_worb).map(|o| cn.grouped_count(cs, o)).collect();
            format!("invertidos={inv:?} agrupados={grp:?}")
        };
        'agrupar: loop {
        'wings: for oi in 0..n_worb {
            let mut guard = 0;
            let mut kicks = 0usize;
            let mut flips_usados = 0usize;
            // O lote acerta enquanto a posicao esta rica em material e depois
            // so falha — e cada falha paga a varredura inteira (MEDIDO: 86
            // acertos contra ~400 falhas, e o 7x7 em 10s onde a base fazia
            // 3.6s). Na primeira falha, para de tentar nesta orbita.
            let mut lote_vivo = true;
            loop {
                let cs = cn.cstate_of(&state);
                let count = cn.grouped_count(&cs, oi);
                if count == 12 {
                    break;
                }
                say(
                    &format!("Agrupando arestas ({count}/12)"),
                    stages.iter().map(|s| s.tokens.len()).sum(),
                );
                guard += 1;
                if guard > 300 {
                    wing_deadlock = Some(oi);
                    break 'wings;
                }

                // MEDIDO: o 3x3 exige um numero PAR de pares invertidos na
                // orbita 0 (com impar acusa "uma aresta esta invertida"). Mas
                // NAO da para escolher a orientacao ao fechar a ultima aresta:
                // cada asa tem quiralidade fixa, entao quando restam duas a
                // orientacao ja esta determinada. Exigir isso aqui so tornava o
                // objetivo impossivel — o solver gastava 103s por tentativa
                // fracassada. A correcao da paridade fica para depois do
                // agrupamento (ver o tratamento do erro do mapa 3x3).
                let goal_any = |s: &SN| {
                    cn.c_centers_solved(s, n_corb, &[]) && cn.grouped_count(s, oi) > count
                };
                let h_any = |s: &SN| {
                    let hh = cn.c_center_h(s, n_corb, &[]);
                    if cn.grouped_count(s, oi) > count {
                        return hh;
                    }
                    let mut hp = 255u8;
                    for tt in 0..12 {
                        if !cn.grouped(s, oi, tt) {
                            hp = hp.min(cn.pair_h(s, oi, tt));
                        }
                    }
                    hh.max(if hp == 255 { 0 } else { hp })
                };

                // escada: busca curta, macro completa, pre-giros com macro
                // rasa, fim de jogo com h exato, busca funda
                let t0 = std::time::Instant::now();
                // Com 11 pares formados, as duas asas que faltam JA estao na
                // casa certa (todas as outras estao ocupadas por pares bons):
                // o que resta e orientacao, e nenhuma permutacao resolve.
                // Vai direto para a fatia, que troca a paridade.
                let so_orientacao = count == 11;
                let mut step = "curta";
                let mut found = None;
                // FATIA EM LOTE, o metodo humano: sanduiche fatia + giros
                // externos + desfaz, exigindo que feche DUAS arestas de uma
                // vez. Os centros ficam intactos por construcao (giros
                // externos nao tocam centro de outra face), e as outras
                // orbitas sao protegidas no objetivo. Profundidade curta de
                // proposito: 2-3 giros externos = 4-5 movimentos por 2 pares,
                // ~2.5 mov/par contra 16.5 do 3-ciclo cirurgico.
                if lote_vivo && !so_orientacao && count <= 10 {
                    let antes: Vec<usize> =
                        (0..n_worb).map(|o| cn.grouped_count(&cs, o)).collect();
                    let goal_lote = |s: &SN| {
                        cn.c_centers_solved(s, n_corb, &[])
                            && cn.grouped_count(s, oi) >= count + 2
                            && (0..n_worb).all(|o| cn.grouped_count(s, o) >= antes[o])
                    };
                    for prof in 2..=3usize {
                        found = cn.slice_face_macro_camada(&cs, &goal_lote, prof, Some(oi + 1));
                        if found.is_some() {
                            step = "fatia-lote";
                            break;
                        }
                    }
                    if found.is_none() {
                        lote_vivo = false;
                    }
                }
                if found.is_none() {
                    step = "curta";
                    found = if so_orientacao {
                        cn.constructive_wing_step(&cs, oi)
                    } else {
                        NSearch::run(&cn, &cs, &goal_any, &h_any, 4)
                    };
                }
                if so_orientacao && found.is_none() {
                    // Faltando so a orientacao do ultimo par: a sequencia
                    // certificada troca essa relacao SEM mexer nos centros,
                    // entao da para consertar aqui e reagrupar — bem mais
                    // barato que voltar ao inicio (que refaz os centros).
                    if flips_usados < 3 {
                        if let Some(alg) = cn.flip_alg.clone() {
                            flips_usados += 1;
                            if dbg >= 1 {
                                eprintln!(
                                    "asas orbita {oi}: orientação -> correção no lugar ({})",
                                    diag(&cn, &cs)
                                );
                            }
                            push_stage(
                                &cn,
                                &mut state,
                                &mut states,
                                &mut stages,
                                "Paridade das arestas".into(),
                                "Inverte um par de asas; sem isso o cubo reduzido seria impossível."
                                    .into(),
                                &alg,
                            );
                            continue;
                        }
                    }
                    // sem correcao no lugar (cubo impar): refaz do inicio
                    if dbg >= 1 {
                        eprintln!("asas orbita {oi}: paridade de orientação -> refazer do inicio");
                    }
                    wing_deadlock = Some(oi);
                    break 'wings;
                }
                // construcao garantida: 3-ciclos cirurgicos de asas (dois
                // deles fecham um par). Vem antes das enumeracoes genericas,
                // que custavam minutos por tentativa e nem sempre achavam.
                if found.is_none() {
                    step = "3-ciclo construido";
                    found = cn.constructive_wing_step(&cs, oi);
                }
                if found.is_none() {
                    // fatia, encaixa (giros de face), desfaz
                    step = "fatia-encaixa";
                    for prof in 2..=4usize {
                        found = cn.slice_face_macro(&cs, &goal_any, prof);
                        if found.is_some() {
                            break;
                        }
                    }
                }
                if found.is_none() {
                    step = "macro2";
                    found = cn
                        .macro_search(&cs, &goal_any, &all_bs, 2, None)
                        .map(|r| cn.trim_tail(&cs, r, &goal_any));
                }
                // Degraus caros, mas eram eles que fechavam a maioria das
                // arestas: o passo construido e um atalho, nao um substituto.
                if found.is_none() {
                    step = "macro3";
                    found = cn
                        .macro_search(&cs, &goal_any, &all_bs, 3, None)
                        .map(|r| cn.trim_tail(&cs, r, &goal_any));
                }
                if found.is_none() {
                    step = "pre-movimento";
                    let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
                    for &t in &cn.premove_order() {
                        let st = cn.capply(&cs, t);
                        let free = t % (cn.depths * 3) < 3;
                        let post = if free { None } else { Some(inv(t)) };
                        if let Some(mut r) = cn.macro_search(&st, &goal_any, &all_bs, 2, post) {
                            let mut seq = vec![t];
                            seq.append(&mut r);
                            if let Some(pm) = post {
                                seq.push(pm);
                            }
                            found = Some(cn.trim_tail(&cs, seq, &goal_any));
                            break;
                        }
                    }
                }
                if found.is_none() {
                    step = "fatia-encaixa-funda";
                    for prof in 5..=5usize {
                        found = cn.slice_face_macro(&cs, &goal_any, prof);
                        if found.is_some() {
                            break;
                        }
                    }
                }
                // fim de jogo (<=3 soltas): a tabela exata de dois pares poda bem
                if found.is_none() {
                    let soltos: Vec<usize> =
                        (0..12).filter(|&tt| !cn.grouped(&cs, oi, tt)).collect();
                    if soltos.len() <= 3 {
                        step = "fim-de-jogo";
                        let goal = |s: &SN| {
                            cn.c_centers_solved(s, n_corb, &[]) && cn.grouped_count(s, oi) == 12
                        };
                        let h = |s: &SN| {
                            let mut hh = cn.c_center_h(s, n_corb, &[]);
                            let so: Vec<usize> =
                                (0..12).filter(|&tt| !cn.grouped(s, oi, tt)).collect();
                            match so.len() {
                                0 => {}
                                1 => hh = hh.max(cn.pair_h(s, oi, so[0])),
                                _ => {
                                    for i in 0..so.len() {
                                        for j in (i + 1)..so.len() {
                                            hh = hh.max(cn.pair2_h(s, oi, so[i], so[j]));
                                        }
                                    }
                                }
                            }
                            hh
                        };
                        found = NSearch::run(&cn, &cs, &goal, &h, 7);
                    }
                }
                // Sem solucao curta: em vez de busca profunda, um "chute" que
                // quebra pares de proposito. O agrupamento guloso refaz cada
                // aresta em ~0,1s, e o chute muda o caso travado. Custa
                // movimentos extras — aceitavel para cubos grandes.
                let mut chutou = false;
                if found.is_none() {
                    step = "chute";
                    let k = kicks;
                    kicks += 1;
                    let sl = 1 + (k % (cn.depths - 1)); // fatia (nunca a face)
                    let fa = (k / (cn.depths - 1)) % 6; // face do meio
                    let ax = (k / ((cn.depths - 1) * 6)) % 6; // face da fatia
                    let s1 = (ax * cn.depths + sl) * 3;
                    let s2 = (fa * cn.depths) * 3;
                    let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
                    let seq = vec![s1, s2, inv(s1)];
                    push_stage(
                        &cn,
                        &mut state,
                        &mut states,
                        &mut stages,
                        "Desembaraçar arestas".into(),
                        "Solta algumas arestas de propósito para sair de um caso travado."
                            .into(),
                        &seq,
                    );
                    chutou = true;
                }
                if dbg >= 2 {
                    eprintln!(
                        "[asas #{guard}] orbita {oi} agrupadas {count}/12 via {step}: {}\n  \
                         estado: {}",
                        found
                            .as_ref()
                            .map(|q| q.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" "))
                            .unwrap_or_else(|| "(nada)".into()),
                        cn.render(&state, &letters)
                    );
                } else if dbg >= 1 {
                    // tally: qual degrau resolveu e quanto custou
                    eprintln!(
                        "DEGRAU {step} {:.3}s (orbita {oi}, {count}/12, achou={})",
                        t0.elapsed().as_secs_f64(),
                        found.is_some()
                    );
                }

                match found {
                    Some(seq) => {
                        let before: Vec<usize> =
                            (0..12).filter(|&tt| cn.grouped(&cs, oi, tt)).collect();
                        push_stage(
                            &cn,
                            &mut state,
                            &mut states,
                            &mut stages,
                            String::new(),
                            String::new(),
                            &seq,
                        );
                        let cs2 = cn.cstate_of(&state);
                        let novos: Vec<&str> = (0..12)
                            .filter(|&tt| cn.grouped(&cs2, oi, tt) && !before.contains(&tt))
                            .map(|tt| EDGE_NAMES_N[tt])
                            .collect();
                        if dbg >= 1 {
                            // tabulavel: quem produz os movimentos das asas
                            eprintln!(
                                "WGASTO step={step} mov={} pares=+{}",
                                seq.len(),
                                novos.len()
                            );
                        }
                        if let Some(st) = stages.last_mut() {
                            st.name = format!("Agrupar aresta {}", novos.join(" e "));
                            st.info =
                                "Junte as fatias dessa aresta (fatia, encaixa, desfaz).".into();
                        }
                    }
                    None if chutou => {} // o chute ja mexeu no estado
                    None => {
                        wing_deadlock = Some(oi);
                        break 'wings;
                    }
                }
            }
        }

        // Agrupou tudo: a paridade dos pares invertidos precisa ser PAR, senao
        // o 3x3 recusa ("uma aresta esta invertida"). A sequencia certificada
        // troca essa paridade sem estragar os centros.
        rodadas += 1;
        if wing_deadlock.is_none() && rodadas < 4 {
            let cs = cn.cstate_of(&state);
            if cn.invertidos(&cs, 0) % 2 == 1 {
                if let Some(alg) = cn.flip_alg.clone() {
                    if dbg >= 1 {
                        eprintln!(
                            "paridade impar no fim da rodada {rodadas}: correcao certificada ({})",
                            diag(&cn, &cs)
                        );
                    }
                    push_stage(
                        &cn,
                        &mut state,
                        &mut states,
                        &mut stages,
                        "Paridade das arestas".into(),
                        "Inverte um par de asas; sem isso o cubo reduzido seria impossível."
                            .into(),
                        &alg,
                    );
                    continue 'agrupar; // reagrupa o que a correcao desfez
                }
            }
        }
        break 'agrupar;
        }

        // Deadlock de asas = par trocado, que e uma TRANSPOSICAO: so uma
        // sequencia de sinal IMPAR desfaz. Medido: os impares sao os giros
        // largos (Rw, 3Rw...), nao as fatias puras — a fatia pura e produto de
        // dois largos, logo par, e era por isso que a correcao nao surtia
        // efeito. Um largo mexe nos centros, entao refazemos do inicio.
        if let Some(oi) = wing_deadlock {
            if attempt + 1 == max_attempts {
                return Err("paridade das arestas nao convergiu".into());
            }
            let candidatos = cn.wing_parity_fixes(oi);
            if candidatos.is_empty() {
                return Err("sem sequencia que troque so a paridade dessa orbita".into());
            }
            // Troca a paridade E embaralha um pouco: repetir a mesma correcao
            // recai no mesmo caso, e a partir de um estado diferente o
            // reagrupamento costuma terminar. Refazer e barato (segundos).
            let mut seq = candidatos[attempt % candidatos.len()].clone();
            let mut r = 0x9e37_79b9_7f4a_7c15u64 ^ (attempt as u64 + 1) * 0x1234_5678_9abc_def1;
            for _ in 0..(2 + attempt) {
                r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let m = ((r >> 33) % cn.n_moves as u64) as usize;
                if seq.last().is_some_and(|&l| l / 3 == m / 3) {
                    continue;
                }
                seq.push(m);
            }
            if dbg >= 1 {
                eprintln!(
                    "paridade das asas (orbita {oi}): {}",
                    seq.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" ")
                );
            }
            prefixo.extend(seq);
            continue 'attempt;
        }

        // ---- mapa 3x3 + paridades ---------------------------------------
        if dbg >= 1 {
            let cs = cn.cstate_of(&state);
            // quantos pares estao "invertidos" (bit 1). Num cubo par a orbita 0
            // nao tem referencia, entao esse bit e livre — e o 3x3 so aceita um
            // numero PAR deles.
            let invertidos: Vec<usize> = (0..n_worb)
                .map(|oi| {
                    (0..12)
                        .filter(|&j| (cs.wo[oi] >> (2 * j)) & 1 == 1)
                        .count()
                })
                .collect();
            eprintln!(
                "INVERTIDOS (tentativa {attempt}): {:?} — paridade {:?}",
                invertidos,
                invertidos.iter().map(|c| c % 2).collect::<Vec<_>>()
            );
        }
        say("Resolvendo como 3x3", stages.iter().map(|s| s.tokens.len()).sum());
        let f3 = cn.reduce_to_3x3(&state);
        match crate::facelet::to_cubie(&f3) {
            Ok(cube3) => {
                let sol3 = search::solve(
                    &cube3,
                    t,
                    SolveParams {
                        max_len: 21,
                        target_len: 21,
                        timeout_ms: 2000,
                        min_ms: 0,
                        threads: search::default_threads(),
                    },
                )?;
                let seq: Vec<usize> = sol3
                    .moves
                    .iter()
                    .map(|&m| (m as usize / 3) * (cn.depths * 3) + (m as usize % 3))
                    .collect();
                push_stage(
                    &cn,
                    &mut state,
                    &mut states,
                    &mut stages,
                    "Resolver como 3x3".into(),
                    "Reduzido, o cubo vira um 3x3: só giros externos.".into(),
                    &seq,
                );
                break 'attempt;
            }
            Err(e) => {
                if attempt + 1 == max_attempts {
                    return Err(format!("paridade nao convergiu: {e}"));
                }
                // Aresta invertida no mapa 3x3 = par de asas da orbita 0
                // trocado (paridade OLL dos cubos pares). A correcao precisa
                // ser IMPAR na orbita 0 — a fatia pura e par ali, e era por
                // isso que nao surtia efeito. Vai junto um embaralhamento
                // pequeno, para o reagrupamento partir de outro estado.
                let mut seq = cn
                    .wing_parity_fixes(0)
                    .get(attempt % 4)
                    .cloned()
                    .unwrap_or_else(|| vec![(1 * cn.depths + 1) * 3]);
                let mut r =
                    0xa076_1d64_78bd_642fu64 ^ (attempt as u64 + 7) * 0x9e37_79b9_7f4a_7c15;
                for _ in 0..(2 + attempt % 5) {
                    r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    let m = ((r >> 33) % cn.n_moves as u64) as usize;
                    if seq.last().is_some_and(|&l| l / 3 == m / 3) {
                        continue;
                    }
                    seq.push(m);
                }
                if dbg >= 1 {
                    eprintln!(
                        "paridade do 3x3 ({e}): {}",
                        seq.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" ")
                    );
                }
                // MEDIDO: com uma thread, os 6 casos tiveram 7 recomecos, TODOS
                // por aqui — nenhum por travamento de asas. E aqui que uma
                // melhoria valeria; retomar dos centros prontos ja foi tentado
                // e sai pior (ver o comentario no topo do laco).
                prefixo.extend(seq);
            }
        }
    }

    // Os reinicios acumulam movimentos redundantes; junta o que e da mesma
    // camada em sequencia e descarta o que se cancela. Refaz as etapas com a
    // lista limpa, mantendo os nomes.
    {
        let mut planos: Vec<(usize, usize)> = Vec::new(); // (movimento, etapa)
        for (si, st) in stages.iter().enumerate() {
            for tk in &st.tokens {
                if let Ok(ms) = cn.parse_moves(tk) {
                    for m in ms {
                        planos.push((m, si));
                    }
                }
            }
        }
        // Junta o que e da mesma camada e reordena o que comuta (mesmo eixo):
        // sem reordenar, `R L R'` ficava como estava. Cada movimento carrega o
        // indice da etapa consigo — mapear pelo indice da lista limpa nao
        // funciona (apos o primeiro cancelamento os rotulos deslizavam e a
        // etapa "Resolver como 3x3" exibia 0 movimentos).
        let depths = cn.depths;
        let limpo: Vec<(usize, usize)> = crate::simplify::simplify_com_rotulos(
            &planos,
            |m| m / 3,
            |m| (m / 3 / depths) % 3,
            |c, p| c * 3 + p,
        );
        if limpo.len() < states.len() - 1 {
            let nomes: Vec<(String, String)> =
                stages.iter().map(|s| (s.name.clone(), s.info.clone())).collect();
            let mut novo_estado = cn.parse(input)?.0;
            let mut novos: Vec<StageN> = Vec::new();
            let mut novos_estados = vec![cn.render(&novo_estado, &letters)];
            for (m, si) in limpo {
                cn.apply(&mut novo_estado, m);
                novos_estados.push(cn.render(&novo_estado, &letters));
                let nome = nomes.get(si).map(|x| x.0.clone()).unwrap_or_default();
                match novos.last_mut() {
                    Some(ult) if ult.name == nome => ult.tokens.push(cn.move_name(m)),
                    _ => novos.push(StageN {
                        name: nome,
                        info: nomes.get(si).map(|x| x.1.clone()).unwrap_or_default(),
                        tokens: vec![cn.move_name(m)],
                    }),
                }
            }
            if cn.faces_uniformes(&novo_estado) {
                if dbg >= 1 {
                    eprintln!(
                        "simplificacao: {} -> {} movimentos",
                        states.len() - 1,
                        novos_estados.len() - 1
                    );
                }
                stages = novos;
                states = novos_estados;
                state = novo_estado;
            }
        }
    }

    // Resolvido = seis faces de cor unica. Nao se exige a orientacao original:
    // nos impares os centros do meio so mudam de lugar girando o cubo inteiro,
    // o que este conjunto de movimentos nao faz.
    if !cn.faces_uniformes(&state) {
        return Err("erro interno: o cubo nao fechou".into());
    }
    let length = stages.iter().map(|s| s.tokens.len()).sum();
    Ok(SolveN { stages, states, length })
}

impl CubeN {
    /// Estado reduzido -> planificacao 3x3 (letras U..B).
    /// Cubo resolvido de verdade: cada face com uma cor so.
    pub fn faces_uniformes(&self, state: &[u8]) -> bool {
        let por_face = self.n * self.n;
        (0..6).all(|f| {
            let c0 = state[f * por_face];
            (0..por_face).all(|k| state[f * por_face + k] == c0)
        })
    }

    fn reduce_to_3x3(&self, state: &[u8]) -> String {
        use crate::facelet::{CORNER_FACELET, EDGE_FACELET, FACE_CHARS};
        let mut out = [b'?'; 54];
        // O centro de cada face do 3x3 recebe a cor REAL daquela face: nos
        // impares ela vem do centro do meio, que pode nao coincidir com o
        // indice (o cubo pode ter ficado numa orientacao diferente).
        for f in 0..6 {
            let cor = match self.midge_facelets {
                Some(_) => {
                    let meio = (self.n - 1) / 2;
                    state[p(self.n, f, meio, meio)] as usize
                }
                None => f,
            };
            out[f * 9 + 4] = FACE_CHARS[cor];
        }
        for i in 0..8 {
            for k in 0..3 {
                out[CORNER_FACELET[i][k]] = FACE_CHARS[state[self.corner_facelets[i][k]] as usize];
            }
        }
        // representante da aresta: midge (impar) ou orbita 0 (par)
        let rep: [[usize; 2]; 12] = if let Some(mf) = &self.midge_facelets {
            *mf
        } else {
            let mut r = [[0usize; 2]; 12];
            for j in 0..12 {
                r[j] = self.wing_orbits[0][2 * j];
            }
            r
        };
        for j in 0..12 {
            out[EDGE_FACELET[j][0]] = FACE_CHARS[state[rep[j][0]] as usize];
            out[EDGE_FACELET[j][1]] = FACE_CHARS[state[rep[j][1]] as usize];
        }
        String::from_utf8(out.to_vec()).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Preenchimento guiado: quais cores ainda podem entrar numa casa
// ---------------------------------------------------------------------------

impl CubeN {
    /// Emparelhamento (Kuhn) peca <-> encaixe: toda peca precisa caber em algum
    /// encaixe livre. `cap` e quantas vezes cada TIPO pode ser usado (asas: 2).
    fn casa_tudo(&self, n_slots: usize, cap: usize, cabe: &dyn Fn(usize, usize) -> bool) -> bool {
        let n_pecas = n_slots;
        let mut dono = vec![usize::MAX; n_slots];
        fn tenta(
            p: usize,
            cabe: &dyn Fn(usize, usize) -> bool,
            visto: &mut [bool],
            dono: &mut [usize],
            n_slots: usize,
        ) -> bool {
            for q in 0..n_slots {
                if visto[q] || !cabe(p, q) {
                    continue;
                }
                visto[q] = true;
                if dono[q] == usize::MAX || tenta(dono[q], cabe, visto, dono, n_slots) {
                    dono[q] = p;
                    return true;
                }
            }
            false
        }
        let _ = cap;
        for p in 0..n_pecas {
            let mut visto = vec![false; n_slots];
            if !tenta(p, cabe, &mut visto, &mut dono, n_slots) {
                return false;
            }
        }
        true
    }

    /// A pintura parcial ainda pode virar um cubo valido? Checagem sonora (nao
    /// recusa nada possivel) das contagens e do encaixe de cada tipo de peca.
    fn viavel_parcial(&self, f: &[Option<u8>]) -> bool {
        let n = self.n;
        let por_face = n * n;
        for c in 0..6u8 {
            if f.iter().filter(|&&x| x == Some(c)).count() > por_face {
                return false;
            }
        }
        let solved = self.solved();

        // centros do meio (impares): um por face, cores distintas
        if n % 2 == 1 {
            let mid = (n - 1) / 2;
            let mut vistas = Vec::new();
            for face in 0..6 {
                if let Some(c) = f[p(n, face, mid, mid)] {
                    if vistas.contains(&c) {
                        return false;
                    }
                    vistas.push(c);
                }
            }
        }

        // centros: no maximo 4 de cada cor por orbita
        for orbit in &self.center_orbits {
            for c in 0..6u8 {
                if orbit.iter().filter(|&&s| f[s] == Some(c)).count() > 4 {
                    return false;
                }
            }
        }

        // cantos: mesmo maquinario do 3x3 (inclui a quiralidade)
        use crate::facelet::CORNER_COLOR;
        let cf = &self.corner_facelets;
        let mut cand: Vec<Vec<(u8, i8)>> = Vec::with_capacity(8);
        let mut tem_livre = false;
        for slot in 0..8 {
            let pintado = cf[slot].iter().any(|&q| f[q].is_some());
            if !pintado {
                tem_livre = true;
            }
            let mut v = Vec::new();
            for peca in 0..8 {
                for o in 0..3usize {
                    let ok = (0..3).all(|k| match f[cf[slot][(k + o) % 3]] {
                        Some(col) => col as usize == CORNER_COLOR[peca][k],
                        None => true,
                    });
                    if ok {
                        if pintado {
                            v.push((peca as u8, o as i8));
                        } else {
                            v.push((peca as u8, -1));
                            break;
                        }
                    }
                }
            }
            if v.is_empty() {
                return false;
            }
            cand.push(v);
        }
        let sc = crate::partial::achievable(&cand, tem_livre, 3);
        if !sc[0] && !sc[1] {
            return false;
        }

        // arestas do meio (impares): 12 tipos, um de cada
        if let Some(mf) = &self.midge_facelets {
            let cabe = |peca: usize, q: usize| -> bool {
                let canon = (solved[mf[peca][0]], solved[mf[peca][1]]);
                for (a, b) in [(canon.0, canon.1), (canon.1, canon.0)] {
                    let ok0 = f[mf[q][0]].map_or(true, |c| c == a);
                    let ok1 = f[mf[q][1]].map_or(true, |c| c == b);
                    if ok0 && ok1 {
                        return true;
                    }
                }
                false
            };
            if !self.casa_tudo(12, 1, &cabe) {
                return false;
            }
        }

        // asas: em cada orbita, 24 casas e 12 tipos (dois de cada)
        for orbit in &self.wing_orbits {
            let cabe = |peca: usize, q: usize| -> bool {
                let t = peca / 2; // duas asas por tipo
                let canon = (solved[orbit[2 * t][0]], solved[orbit[2 * t][1]]);
                for (a, b) in [(canon.0, canon.1), (canon.1, canon.0)] {
                    let ok0 = f[orbit[q][0]].map_or(true, |c| c == a);
                    let ok1 = f[orbit[q][1]].map_or(true, |c| c == b);
                    if ok0 && ok1 {
                        return true;
                    }
                }
                false
            };
            if !self.casa_tudo(24, 2, &cabe) {
                return false;
            }
        }
        true
    }
}

/// Cores que ainda podem entrar na casa `pos`, dada a pintura parcial (`.` =
/// vazio). E o que o preenchimento guiado usa para travar o impossivel.
pub fn allowed_colors_n(n: usize, input: &str, pos: usize) -> Result<Vec<usize>, String> {
    let cn = cuben(n);
    let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != cn.n_facelets {
        return Err(format!("esperava {} simbolos, recebi {}", cn.n_facelets, chars.len()));
    }
    if pos >= cn.n_facelets {
        return Err("posicao invalida".into());
    }
    let mut f = vec![None; cn.n_facelets];
    for (i, &c) in chars.iter().enumerate() {
        if c == '.' {
            continue;
        }
        match "URFDLB".chars().position(|x| x == c) {
            Some(k) => f[i] = Some(k as u8),
            None => return Err(format!("simbolo desconhecido '{c}'")),
        }
    }
    let mut out = Vec::new();
    for c in 0..6u8 {
        let mut g = f.clone();
        g[pos] = Some(c);
        if cn.viavel_parcial(&g) {
            out.push(c as usize);
        }
    }
    Ok(out)
}

/// Embaralhamento por movimentos aleatorios do proprio conjunto.
pub fn scramble_n(n: usize, mut rand: impl FnMut(u64) -> u64) -> (String, String) {
    let cn = cuben(n);
    let mut state = cn.solved();
    let mut tokens = Vec::new();
    let mut last = usize::MAX;
    let total = n * 10;
    let mut cnt = 0;
    while cnt < total {
        let m = rand(cn.n_moves as u64) as usize;
        if m / 3 == last {
            continue;
        }
        last = m / 3;
        cn.apply(&mut state, m);
        tokens.push(cn.move_name(m));
        cnt += 1;
    }
    let letters = ['U', 'R', 'F', 'D', 'L', 'B'];
    (cn.render(&state, &letters), tokens.join(" "))
}

/// Aplica notacao sobre a planificacao pintada.
pub fn apply_n(n: usize, input: &str, moves_str: &str) -> Result<String, String> {
    let cn = cuben(n);
    let (mut state, letters) = cn.parse(input)?;
    let seq = cn.parse_moves(moves_str)?;
    cn.apply_seq(&mut state, &seq);
    Ok(cn.render(&state, &letters))
}

impl CubeN {
    pub fn move_name(&self, m: usize) -> String {
        let faces = ["U", "R", "F", "D", "L", "B"];
        let pow = ["", "2", "'"];
        let per_face = self.depths * 3;
        let f = m / per_face;
        let rest = m % per_face;
        let depth = rest / 3 + 1;
        let pw = rest % 3;
        match depth {
            1 => format!("{}{}", faces[f], pow[pw]),
            2 => format!("{}w{}", faces[f], pow[pw]),
            d => format!("{}{}w{}", d, faces[f], pow[pw]),
        }
    }

    pub fn parse_moves(&self, s: &str) -> Result<Vec<usize>, String> {
        let mut out = Vec::new();
        for tok in s.split_whitespace() {
            let t = tok.replace('\u{2019}', "'");
            let (body, pw) = if let Some(x) = t.strip_suffix('\'') {
                (x.to_string(), 2usize)
            } else if let Some(x) = t.strip_suffix('2') {
                (x.to_string(), 1)
            } else {
                (t.clone(), 0)
            };
            let (body, mut depth) = {
                let mut d = 1usize;
                let mut b = body.clone();
                if let Some(first) = b.chars().next() {
                    if first.is_ascii_digit() {
                        d = first.to_digit(10).unwrap() as usize;
                        b = b[1..].to_string();
                    }
                }
                (b, d)
            };
            let (face_str, wide) = if let Some(x) = body.strip_suffix('w') {
                (x.to_uppercase(), true)
            } else if body.len() == 1 && body.chars().next().unwrap().is_lowercase() {
                (body.to_uppercase(), true)
            } else {
                (body.to_uppercase(), false)
            };
            if wide && depth == 1 {
                depth = 2;
            }
            if depth < 1 || depth > self.depths {
                return Err(format!("profundidade invalida em \"{tok}\""));
            }
            let f = FACES
                .iter()
                .position(|&c| c.to_string() == face_str)
                .ok_or_else(|| format!("movimento desconhecido: \"{tok}\""))?;
            out.push(f * self.depths * 3 + (depth - 1) * 3 + pw);
        }
        Ok(out)
    }

    pub fn apply(&self, state: &mut Vec<u8>, m: usize) {
        let perm = &self.moves[m];
        let old = state.clone();
        for s in 0..self.n_facelets {
            state[perm[s] as usize] = old[s];
        }
    }

    pub fn apply_seq(&self, state: &mut Vec<u8>, seq: &[usize]) {
        for &m in seq {
            self.apply(state, m);
        }
    }

    pub fn solved(&self) -> Vec<u8> {
        (0..self.n_facelets).map(|i| (i / (self.n * self.n)) as u8).collect()
    }

    pub fn render(&self, state: &[u8], letters: &[char; 6]) -> String {
        state.iter().map(|&c| letters[c as usize]).collect()
    }

    // -----------------------------------------------------------------
    // Construcao
    // -----------------------------------------------------------------

    fn build(n: usize) -> CubeN {
        // O 4x4 se encaixa na mesma construcao: 2 profundidades, 36 movimentos,
        // uma orbita de centros e uma de asas, sem arestas do meio.
        assert!((4..=7).contains(&n));
        let n_facelets = 6 * n * n;
        // profundidade maxima de wide: inclui a fatia central nos impares
        // (5x5 precisa de 3Rw; sem ela o grupo gerado nao cobre o cubo todo)
        let depths = (n + 1) / 2;
        let (u, rr, f, d, l, b) = (0usize, 1, 2, 3, 4, 5);
        let m = n - 1;

        // ---- movimentos base: rotacao da face + aneis das camadas -------
        let identity: Vec<u16> = (0..n_facelets as u16).collect();
        let compose = |a: &Vec<u16>, bb: &Vec<u16>| -> Vec<u16> {
            let mut r = identity.clone();
            for s in 0..n_facelets {
                r[s] = bb[a[s] as usize];
            }
            r
        };

        let row = |fc: usize, r: usize| -> Vec<usize> { (0..n).map(|i| p(n, fc, r, i)).collect() };
        let row_rev =
            |fc: usize, r: usize| -> Vec<usize> { (0..n).map(|i| p(n, fc, r, m - i)).collect() };
        let col = |fc: usize, c: usize| -> Vec<usize> { (0..n).map(|i| p(n, fc, i, c)).collect() };
        let col_rev =
            |fc: usize, c: usize| -> Vec<usize> { (0..n).map(|i| p(n, fc, m - i, c)).collect() };

        let cycle = |perm: &mut Vec<u16>, strips: [Vec<usize>; 4]| {
            for k in 0..4 {
                let from = &strips[k];
                let to = &strips[(k + 1) % 4];
                for i in 0..n {
                    perm[from[i]] = to[i] as u16;
                }
            }
        };

        // camada L (0-indexada) de cada face, mesma geometria do 4x4
        let layer_ring = |perm: &mut Vec<u16>, face: usize, layer: usize| match face {
            0 => cycle(perm, [row(f, layer), row(l, layer), row(b, layer), row(rr, layer)]),
            1 => cycle(
                perm,
                [col(f, m - layer), col(u, m - layer), col_rev(b, layer), col(d, m - layer)],
            ),
            2 => cycle(
                perm,
                [
                    row(u, m - layer),
                    col(rr, layer),
                    row_rev(d, layer),
                    col_rev(l, m - layer),
                ],
            ),
            3 => cycle(
                perm,
                [row(f, m - layer), row(rr, m - layer), row(b, m - layer), row(l, m - layer)],
            ),
            4 => cycle(perm, [col(u, layer), col(f, layer), col(d, layer), col_rev(b, m - layer)]),
            _ => cycle(
                perm,
                [row(u, layer), col_rev(l, layer), row_rev(d, m - layer), col(rr, m - layer)],
            ),
        };

        let rotate_face = |perm: &mut Vec<u16>, face: usize| {
            for r in 0..n {
                for c in 0..n {
                    perm[p(n, face, r, c)] = p(n, face, c, m - r) as u16;
                }
            }
        };

        // wide de profundidade k = camadas 0..k
        let mut moves: Vec<Vec<u16>> = Vec::new();
        for face in 0..6 {
            for depth in 1..=depths {
                let mut one = identity.clone();
                rotate_face(&mut one, face);
                for layer in 0..depth {
                    layer_ring(&mut one, face, layer);
                }
                let two = compose(&one, &one);
                let three = compose(&two, &one);
                moves.push(one);
                moves.push(two);
                moves.push(three);
            }
        }
        let n_moves = moves.len();

        // ---- pecas -------------------------------------------------------
        let corner_facelets: [[usize; 3]; 8] = [
            [p(n, u, m, m), p(n, rr, 0, 0), p(n, f, 0, m)],
            [p(n, u, m, 0), p(n, f, 0, 0), p(n, l, 0, m)],
            [p(n, u, 0, 0), p(n, l, 0, 0), p(n, b, 0, m)],
            [p(n, u, 0, m), p(n, b, 0, 0), p(n, rr, 0, m)],
            [p(n, d, 0, m), p(n, f, m, m), p(n, rr, m, 0)],
            [p(n, d, 0, 0), p(n, l, m, m), p(n, f, m, 0)],
            [p(n, d, m, 0), p(n, b, m, m), p(n, l, m, 0)],
            [p(n, d, m, m), p(n, rr, m, m), p(n, b, m, 0)],
        ];

        // pares de adesivos de aresta no deslocamento k (0-indexado a partir
        // do canto: posicao k e n-1-k na aresta), mesma geometria do 4x4
        let edge_pair_at = |k: usize| -> [[usize; 2]; 24] {
            let q = m - k;
            [
                [p(n, u, k, m), p(n, rr, 0, q)],
                [p(n, u, q, m), p(n, rr, 0, k)],
                [p(n, u, m, k), p(n, f, 0, k)],
                [p(n, u, m, q), p(n, f, 0, q)],
                [p(n, u, k, 0), p(n, l, 0, k)],
                [p(n, u, q, 0), p(n, l, 0, q)],
                [p(n, u, 0, k), p(n, b, 0, q)],
                [p(n, u, 0, q), p(n, b, 0, k)],
                [p(n, d, k, m), p(n, rr, m, k)],
                [p(n, d, q, m), p(n, rr, m, q)],
                [p(n, d, 0, k), p(n, f, m, k)],
                [p(n, d, 0, q), p(n, f, m, q)],
                [p(n, d, k, 0), p(n, l, m, q)],
                [p(n, d, q, 0), p(n, l, m, k)],
                [p(n, d, m, k), p(n, b, m, q)],
                [p(n, d, m, q), p(n, b, m, k)],
                [p(n, f, k, m), p(n, rr, k, 0)],
                [p(n, f, q, m), p(n, rr, q, 0)],
                [p(n, f, k, 0), p(n, l, k, m)],
                [p(n, f, q, 0), p(n, l, q, m)],
                [p(n, b, k, m), p(n, l, k, 0)],
                [p(n, b, q, m), p(n, l, q, 0)],
                [p(n, b, k, 0), p(n, rr, k, m)],
                [p(n, b, q, 0), p(n, rr, q, m)],
            ]
        };

        // orbitas de asas: deslocamentos 1..=(n-2 - impar)/2
        let n_worbits = (n - 2) / 2;
        let mut wing_orbits = Vec::new();
        for o in 1..=n_worbits {
            wing_orbits.push(edge_pair_at(o));
        }

        // midges (impar): o par central de cada aresta com k = n/2, colapsado
        let midge_facelets: Option<[[usize; 2]; 12]> = if n % 2 == 1 {
            let k = n / 2;
            let pairs = edge_pair_at(k); // aqui k == q: cada par vira 1 midge
            let mut mf = [[0usize; 2]; 12];
            for j in 0..12 {
                mf[j] = pairs[2 * j];
            }
            Some(mf)
        } else {
            None
        };

        // orbitas de centros: classes (i,j) do bloco (n-2)x(n-2) sob rotacao
        let mut center_orbits: Vec<[usize; 24]> = Vec::new();
        {
            let inner = n - 2;
            let mut seen = vec![false; inner * inner];
            for i in 0..inner {
                for j in 0..inner {
                    if seen[i * inner + j] {
                        continue;
                    }
                    // rotacoes de (i,j) no bloco interno
                    let mut pos = (i, j);
                    let mut cls = Vec::new();
                    for _ in 0..4 {
                        if !seen[pos.0 * inner + pos.1] {
                            seen[pos.0 * inner + pos.1] = true;
                            cls.push(pos);
                        }
                        pos = (pos.1, inner - 1 - pos.0);
                    }
                    if cls.len() == 1 {
                        continue; // centro fixo do meio (impar)
                    }
                    // classe com 2 posicoes (diagonal do meio em N par? nao
                    // ocorre: classes tem 4 ou 1) — garante 4
                    assert_eq!(cls.len(), 4, "classe de centro inesperada");
                    let mut slots = [0usize; 24];
                    for face in 0..6 {
                        for (t, &(r, c)) in cls.iter().enumerate() {
                            slots[face * 4 + t] = p(n, face, r + 1, c + 1);
                        }
                    }
                    center_orbits.push(slots);
                }
            }
        }

        // ---- mapas de movimento por orbita ------------------------------
        let mut cmove = Vec::new();
        for orbit in &center_orbits {
            let mut per_move = Vec::with_capacity(n_moves);
            for mvp in &moves {
                let mut mm = vec![0u8; 24];
                for (i, &s) in orbit.iter().enumerate() {
                    let dst = mvp[s] as usize;
                    let j = orbit
                        .iter()
                        .position(|&x| x == dst)
                        .expect("centro sai da propria orbita");
                    mm[i] = j as u8;
                }
                per_move.push(mm);
            }
            cmove.push(per_move);
        }

        let mut wmove = Vec::new();
        let mut wflip = Vec::new();
        for orbit in &wing_orbits {
            let mut per_move = Vec::with_capacity(n_moves);
            let mut per_flip = Vec::with_capacity(n_moves);
            for mvp in &moves {
                let mut wm = vec![0u8; 24];
                let mut wf = 0u32;
                for (i, w) in orbit.iter().enumerate() {
                    let (a, bb) = (mvp[w[0]] as usize, mvp[w[1]] as usize);
                    let j = orbit
                        .iter()
                        .position(|x| (x[0] == a && x[1] == bb) || (x[0] == bb && x[1] == a))
                        .expect("asa sai da propria orbita");
                    wm[i] = j as u8;
                    if orbit[j][0] == bb {
                        wf |= 1 << i;
                    }
                }
                per_move.push(wm);
                per_flip.push(wf);
            }
            wmove.push(per_move);
            wflip.push(per_flip);
        }

        let mut mmove: Vec<[(u8, u8); 12]> = Vec::new();
        if let Some(mf) = &midge_facelets {
            for mvp in &moves {
                let mut mm = [(0u8, 0u8); 12];
                for (i, w) in mf.iter().enumerate() {
                    let (a, bb) = (mvp[w[0]] as usize, mvp[w[1]] as usize);
                    let j = mf
                        .iter()
                        .position(|x| (x[0] == a && x[1] == bb) || (x[0] == bb && x[1] == a))
                        .expect("midge vira midge");
                    mm[i] = (j as u8, if mf[j][0] == bb { 1 } else { 0 });
                }
                mmove.push(mm);
            }
        }

        // ---- tabelas exatas por orbita ----------------------------------
        let center_dist: Vec<[Vec<u8>; 6]> = cmove
            .iter()
            .map(|cm| {
                std::array::from_fn(|face| {
                    let mut dist = vec![255u8; 10626];
                    let home: [u8; 4] =
                        [4 * face as u8, 4 * face as u8 + 1, 4 * face as u8 + 2, 4 * face as u8 + 3];
                    dist[subset_rank(&home)] = 0;
                    let mut frontier = vec![home];
                    let mut dd = 0u8;
                    while !frontier.is_empty() {
                        let mut next = Vec::new();
                        for st in &frontier {
                            for mv in cm {
                                let mut v = [
                                    mv[st[0] as usize],
                                    mv[st[1] as usize],
                                    mv[st[2] as usize],
                                    mv[st[3] as usize],
                                ];
                                v.sort_unstable();
                                let i = subset_rank(&v);
                                if dist[i] == 255 {
                                    dist[i] = dd + 1;
                                    next.push(v);
                                }
                            }
                        }
                        frontier = next;
                        dd += 1;
                    }
                    dist
                })
            })
            .collect();

        let midmove: Vec<[u8; 6]> = moves
            .iter()
            .map(|mvp| {
                let mut mm = [0u8; 6];
                if n % 2 == 1 {
                    let mid = (n - 1) / 2;
                    for f in 0..6 {
                        let dst = mvp[p(n, f, mid, mid)] as usize;
                        let g = (0..6)
                            .position(|x| p(n, x, mid, mid) == dst)
                            .expect("centro do meio sai do meio");
                        mm[f] = g as u8;
                    }
                } else {
                    for f in 0..6 {
                        mm[f] = f as u8;
                    }
                }
                mm
            })
            .collect();

        let center_pos_dist: Vec<[[u8; 24]; 6]> = cmove
            .iter()
            .map(|cm| {
                std::array::from_fn(|face| {
                    let mut dist = [255u8; 24];
                    let mut frontier: Vec<usize> = (4 * face..4 * face + 4).collect();
                    for &p in &frontier {
                        dist[p] = 0;
                    }
                    let mut dd = 0u8;
                    while !frontier.is_empty() {
                        let mut next = Vec::new();
                        for &p in &frontier {
                            for mv in cm {
                                let q = mv[p] as usize;
                                if dist[q] == 255 {
                                    dist[q] = dd + 1;
                                    next.push(q);
                                }
                            }
                        }
                        frontier = next;
                        dd += 1;
                    }
                    dist
                })
            })
            .collect();

        let pair_dist: Vec<Vec<u8>> = wmove
            .iter()
            .zip(wflip.iter())
            .map(|(wm, wf)| {
                let idx = |a: usize, bb: usize, rel: usize| (a * 24 + bb) * 2 + rel;
                let mut dist = vec![255u8; 24 * 24 * 2];
                let mut frontier = Vec::new();
                for j in 0..12 {
                    for (a, bb) in [(2 * j, 2 * j + 1), (2 * j + 1, 2 * j)] {
                        dist[idx(a, bb, 0)] = 0;
                        frontier.push((a as u8, bb as u8, 0u8));
                    }
                }
                let mut dd = 0u8;
                while !frontier.is_empty() {
                    let mut next = Vec::new();
                    for &(a, bb, rel) in &frontier {
                        for mi in 0..wm.len() {
                            let (a2, b2) = (wm[mi][a as usize], wm[mi][bb as usize]);
                            let fa = (wf[mi] >> a) & 1;
                            let fb = (wf[mi] >> bb) & 1;
                            let rel2 = (rel as u32 ^ fa ^ fb) as u8;
                            let i = idx(a2 as usize, b2 as usize, rel2 as usize);
                            if dist[i] == 255 {
                                dist[i] = dd + 1;
                                next.push((a2, b2, rel2));
                            }
                        }
                    }
                    frontier = next;
                    dd += 1;
                }
                dist
            })
            .collect();

        let pair2_dist: Vec<Vec<u8>> = wmove
            .iter()
            .zip(wflip.iter())
            .map(|(wm, wf)| {
                let idx = |a1: usize, b1: usize, r1: usize, a2: usize, b2: usize, r2: usize| {
                    (((a1 * 24 + b1) * 2 + r1) * 576 + (a2 * 24 + b2)) * 2 + r2
                };
                let mut dist = vec![255u8; 576 * 2 * 576 * 2];
                let mut frontier: Vec<(u8, u8, u8, u8, u8, u8)> = Vec::new();
                for j1 in 0..12usize {
                    for (a1, b1) in [(2 * j1, 2 * j1 + 1), (2 * j1 + 1, 2 * j1)] {
                        for j2 in 0..12usize {
                            if j2 == j1 {
                                continue;
                            }
                            for (a2, b2) in [(2 * j2, 2 * j2 + 1), (2 * j2 + 1, 2 * j2)] {
                                let i = idx(a1, b1, 0, a2, b2, 0);
                                if dist[i] == 255 {
                                    dist[i] = 0;
                                    frontier.push((a1 as u8, b1 as u8, 0, a2 as u8, b2 as u8, 0));
                                }
                            }
                        }
                    }
                }
                let mut dd = 0u8;
                while !frontier.is_empty() {
                    let mut next = Vec::new();
                    for &(a1, b1, r1, a2, b2, r2) in &frontier {
                        for mi in 0..wm.len() {
                            let fl = |q: u8| ((wf[mi] >> q) & 1) as u8;
                            let na1 = wm[mi][a1 as usize];
                            let nb1 = wm[mi][b1 as usize];
                            let na2 = wm[mi][a2 as usize];
                            let nb2 = wm[mi][b2 as usize];
                            let nr1 = r1 ^ fl(a1) ^ fl(b1);
                            let nr2 = r2 ^ fl(a2) ^ fl(b2);
                            let i = idx(
                                na1 as usize,
                                nb1 as usize,
                                nr1 as usize,
                                na2 as usize,
                                nb2 as usize,
                                nr2 as usize,
                            );
                            if dist[i] == 255 {
                                dist[i] = dd + 1;
                                next.push((na1, nb1, nr1, na2, nb2, nr2));
                            }
                        }
                    }
                    frontier = next;
                    dd += 1;
                }
                dist
            })
            .collect();

        let mut cn = CubeN {
            n,
            n_facelets,
            moves,
            n_moves,
            depths,
            corner_facelets,
            midge_facelets,
            wing_orbits,
            center_orbits,
            cmove,
            wmove,
            wflip,
            mmove,
            center_dist,
            center_pos_dist,
            midmove,
            pair_dist,
            pair2_dist,
            base3: Vec::new(),
            base3_fontes: Vec::new(),
            triple_trees: Vec::new(),
            wbase3: Vec::new(),
            wbase3_fontes: Vec::new(),
            wing_trees: Vec::new(),
            flip_alg: None,
        };
        // 3-ciclos puros e arvores de conjugacao (precisam do resto pronto).
        // A busca custa ~30s somando os tres tamanhos, e o resultado e fixo por
        // N: vale cachear em disco. O cache e SEMPRE reverificado (suporte 3 na
        // orbita certa), entao um arquivo velho ou corrompido nao passa.
        let cache = std::env::temp_dir().join(format!("cubo-solver-3ciclos-{n}.txt"));
        let (base3, wbase3) = ler_cache_3ciclos(&cache, &cn).unwrap_or_else(|| {
            let b = cn.find_base_3cycles();
            let w = cn.find_base_wing_3cycles();
            if let Err(e) = gravar_cache_3ciclos(&cache, &b, &w) {
                eprintln!("aviso: nao salvei o cache de 3-ciclos: {e}");
            }
            (b, w)
        });
        cn.base3 = base3;
        cn.base3_fontes = (0..cn.center_orbits.len())
            .map(|oi| match &cn.base3[oi] {
                Some(seq) => cn.variantes_do_ciclo(seq, true, oi),
                None => Vec::new(),
            })
            .collect();
        cn.triple_trees = (0..cn.center_orbits.len())
            .map(|oi| {
                if cn.base3_fontes[oi].is_empty() {
                    Vec::new()
                } else {
                    cn.triple_tree_multi(oi, &cn.base3_fontes[oi], false)
                }
            })
            .collect();
        cn.wbase3 = wbase3;
        // Cubos pares: acha (e CERTIFICA por simulacao) uma sequencia que troca
        // a paridade dos pares invertidos sem estragar os centros. Sem ela, os
        // cubos com paridade impar nao fecham — o 3x3 recusa a reducao.
        if n % 2 == 0 {
            let base = cn.cstate_of(&cn.solved());
            // O criterio e frouxo de proposito: centros intactos e paridade
            // trocada na orbita 0. No 6x6 isso aceita uma sequencia que inverte
            // METADE da aresta — as orbitas ficam em desacordo e a aresta
            // desagrupa (12 -> 11).
            //
            // MEDIDO, e contra a intuicao: exigir a aresta INTEIRA (largura 3 no
            // 6x6, que vira as duas orbitas de uma vez) piora 6 casos de 218s
            // para 464s, e ainda alonga as solucoes (5480 -> 5840 movimentos).
            // Medicao deterministica, com CUBEN_WORKERS=1 dos dois lados — a
            // primeira versao deste numero (38s contra 96s) saiu de rodadas
            // paralelas, que variam sozinhas.  A explicacao esta no diagnostico:
            // com 11 pares
            // formados, o que destrava o agrupamento e a PERTURBACAO, nao a
            // troca de paridade. A sequencia coerente deixa os agrupamentos
            // exatamente como estavam (medido: nunca passou de 11/12, nem
            // conjugada por giros de face ate profundidade 3), entao o solver
            // cai no caminho caro de refazer tudo do inicio. A que "estraga" um
            // par muda a configuracao e o agrupamento acha saida.
            //
            // Fica registrado para nao se tentar de novo: ver
            // `retrato_da_correcao_de_paridade` para o que cada uma faz.
            //
            // `CUBEN_FLIP_INTEIRO=1` liga a variante coerente, para refazer a
            // medicao (de preferencia com CUBEN_WORKERS=1, senao e ruido).
            let inteiro = std::env::var("CUBEN_FLIP_INTEIRO").is_ok();
            let n_worb = cn.wing_orbits.len();
            let com_largura = |txt: &str, w: usize| -> String {
                txt.split_whitespace()
                    .map(|t| {
                        if t.contains('w') && w > 2 {
                            format!("{w}{t}")
                        } else {
                            t.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            'certifica: for w in 2..=n / 2 {
                for txt in [
                    "Rw' U2 Lw F2 Lw' F2 Rw2 U2 Rw U2 Rw' U2 F2 Rw2 F2",
                    "Rw2 B2 U2 Lw U2 Rw' U2 Rw U2 F2 Rw F2 Lw' B2 Rw2",
                ] {
                    let Ok(seq) = cn.parse_moves(&com_largura(txt, w)) else { continue };
                    let mut s = base;
                    for &m in &seq {
                        s = cn.capply(&s, m);
                    }
                    let centros_intactos = s.cent == base.cent;
                    let trocou_paridade = if inteiro {
                        // a aresta vira por inteiro: nenhuma orbita desagrupa e
                        // a paridade troca em todas elas
                        (0..n_worb).all(|o| cn.grouped_count(&s, o) == 12)
                            && (0..n_worb).all(|o| cn.invertidos(&s, o) % 2 == 1)
                    } else {
                        cn.invertidos(&s, 0) % 2 == 1
                    };
                    if centros_intactos && trocou_paridade {
                        cn.flip_alg = Some(seq);
                        break 'certifica;
                    }
                }
            }
        }
        cn.wbase3_fontes = (0..cn.wing_orbits.len())
            .map(|oi| match &cn.wbase3[oi] {
                Some(seq) => cn.variantes_do_ciclo(seq, false, oi),
                None => Vec::new(),
            })
            .collect();
        cn.wing_trees = (0..cn.wing_orbits.len())
            .map(|oi| {
                if cn.wbase3_fontes[oi].is_empty() {
                    Vec::new()
                } else {
                    cn.triple_tree_multi(oi, &cn.wbase3_fontes[oi], true)
                }
            })
            .collect();
        cn
    }
}

// ---------------------------------------------------------------------------
// Parse + esquema de cores
// ---------------------------------------------------------------------------

impl CubeN {
    /// Interpreta a planificacao: valida pecas e normaliza cores -> faces.
    /// Impar: esquema pelos centros fixos do meio. Par: ancora no canto DBL.
    pub fn parse(&self, input: &str) -> Result<(Vec<u8>, [char; 6]), String> {
        let n = self.n;
        let chars: Vec<char> = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
        if chars.len() != self.n_facelets {
            return Err(format!(
                "esperava {} adesivos, recebi {}",
                self.n_facelets,
                chars.len()
            ));
        }
        let mut colors: Vec<char> = Vec::new();
        for &c in &chars {
            if !colors.contains(&c) {
                colors.push(c);
            }
        }
        if colors.len() != 6 {
            return Err(format!("esperava 6 cores, encontrei {}", colors.len()));
        }
        for &c in &colors {
            let cnt = chars.iter().filter(|&&x| x == c).count();
            if cnt != n * n {
                return Err(format!("a cor '{c}' aparece {cnt} vezes (deveriam ser {})", n * n));
            }
        }
        let idx_of = |c: char| colors.iter().position(|&x| x == c).unwrap();
        let raw: Vec<usize> = chars.iter().map(|&c| idx_of(c)).collect();

        // esquema
        let scheme: [usize; 6] = if n % 2 == 1 {
            let mid = n / 2;
            let mut s = [0usize; 6];
            for face in 0..6 {
                s[face] = raw[p(n, face, mid, mid)];
            }
            let mut set = s.to_vec();
            set.sort_unstable();
            set.dedup();
            if set.len() != 6 {
                return Err("os centros do meio precisam ter 6 cores distintas".into());
            }
            s
        } else {
            // ancora DBL, opostos por adjacencia dos cantos (como no 4x4)
            let cf = &self.corner_facelets;
            let d_col = raw[cf[6][0]];
            let b_col = raw[cf[6][1]];
            let l_col = raw[cf[6][2]];
            let mut adj = [[false; 6]; 6];
            for t in cf.iter() {
                let (a, bb, c) = (raw[t[0]], raw[t[1]], raw[t[2]]);
                for (x, y) in [(a, bb), (a, c), (bb, c)] {
                    adj[x][y] = true;
                    adj[y][x] = true;
                }
            }
            let opposite = |x: usize| -> Result<usize, String> {
                let mut op = None;
                for y in 0..6 {
                    if y != x && !adj[x][y] {
                        if op.is_some() {
                            return Err("os cantos nao formam um cubo real".into());
                        }
                        op = Some(y);
                    }
                }
                op.ok_or_else(|| "os cantos nao formam um cubo real".into())
            };
            [opposite(d_col)?, opposite(l_col)?, opposite(b_col)?, d_col, l_col, b_col]
        };
        let mut face_of = [usize::MAX; 6];
        for (face, &col) in scheme.iter().enumerate() {
            face_of[col] = face;
        }
        if face_of.iter().any(|&x| x == usize::MAX) {
            return Err("as cores nao formam um esquema valido".into());
        }
        let state: Vec<u8> = raw.iter().map(|&c| face_of[c] as u8).collect();
        let letters: [char; 6] = std::array::from_fn(|face| colors[scheme[face]]);

        // ---- validacao de pecas -----------------------------------------
        let solved = self.solved();
        // cantos (a menos de rotacao ciclica)
        let norm3 = |t: [u8; 3]| {
            let rots = [t, [t[1], t[2], t[0]], [t[2], t[0], t[1]]];
            *rots.iter().min().unwrap()
        };
        let mut tri: Vec<[u8; 3]> = self
            .corner_facelets
            .iter()
            .map(|t| norm3([state[t[0]], state[t[1]], state[t[2]]]))
            .collect();
        let mut want: Vec<[u8; 3]> = self
            .corner_facelets
            .iter()
            .map(|t| norm3([solved[t[0]], solved[t[1]], solved[t[2]]]))
            .collect();
        tri.sort_unstable();
        want.sort_unstable();
        if tri != want {
            return Err("os cantos nao formam um conjunto valido de pecas".into());
        }
        // midges e asas: pares nao ordenados
        let pair_of = |s: &[u8], w: &[usize; 2]| {
            let (a, bb) = (s[w[0]], s[w[1]]);
            if a <= bb {
                (a, bb)
            } else {
                (bb, a)
            }
        };
        if let Some(mf) = &self.midge_facelets {
            let mut have: Vec<_> = mf.iter().map(|w| pair_of(&state, w)).collect();
            let mut wantp: Vec<_> = mf.iter().map(|w| pair_of(&solved, w)).collect();
            have.sort_unstable();
            wantp.sort_unstable();
            if have != wantp {
                return Err("as pecas centrais das arestas nao batem".into());
            }
        }
        for orbit in &self.wing_orbits {
            let mut have: Vec<_> = orbit.iter().map(|w| pair_of(&state, w)).collect();
            let mut wantp: Vec<_> = orbit.iter().map(|w| pair_of(&solved, w)).collect();
            have.sort_unstable();
            wantp.sort_unstable();
            if have != wantp {
                return Err("as pecas de aresta nao formam um conjunto valido".into());
            }
        }
        for orbit in &self.center_orbits {
            for face in 0..6u8 {
                let cnt = orbit.iter().filter(|&&s| state[s] == face).count();
                if cnt != 4 {
                    return Err("os centros nao formam um conjunto valido".into());
                }
            }
        }
        Ok((state, letters))
    }
}

// ---------------------------------------------------------------------------
// Estado compacto (todas as orbitas + midges) e busca com raiz paralela
// ---------------------------------------------------------------------------

const MAX_CORB: usize = 6;
const MAX_WORB: usize = 2;

#[derive(Clone, Copy, PartialEq)]
struct SN {
    cent: [u8; 24 * MAX_CORB],
    wt: [u8; 24 * MAX_WORB],
    wo: [u32; MAX_WORB],
    mt: [u8; 12],
    mo: u16,
    /// cor no centro do meio de cada face (so muda em cubos impares)
    mid: [u8; 6],
}

impl CubeN {
    fn type_map(&self) -> [[u8; 6]; 6] {
        let solved = self.solved();
        let orbit = &self.wing_orbits[0];
        let mut t = [[255u8; 6]; 6];
        for k in 0..12 {
            let (a, bb) = (solved[orbit[2 * k][0]], solved[orbit[2 * k][1]]);
            t[a as usize][bb as usize] = k as u8;
            t[bb as usize][a as usize] = k as u8;
        }
        t
    }

    fn cstate_of(&self, state: &[u8]) -> SN {
        let solved = self.solved();
        let tmap = self.type_map();
        let mut s = SN {
            cent: [0; 24 * MAX_CORB],
            wt: [0; 24 * MAX_WORB],
            wo: [0; MAX_WORB],
            mt: [0; 12],
            mo: 0,
            mid: [0, 1, 2, 3, 4, 5],
        };
        if self.n % 2 == 1 {
            let mid = (self.n - 1) / 2;
            for f in 0..6 {
                s.mid[f] = state[p(self.n, f, mid, mid)];
            }
        }
        for (oi, orbit) in self.center_orbits.iter().enumerate() {
            for (i, &slot) in orbit.iter().enumerate() {
                s.cent[oi * 24 + i] = state[slot];
            }
        }
        for (oi, orbit) in self.wing_orbits.iter().enumerate() {
            for (q, w) in orbit.iter().enumerate() {
                let shown = (state[w[0]], state[w[1]]);
                let t = tmap[shown.0 as usize][shown.1 as usize] as usize;
                s.wt[oi * 24 + q] = t as u8;
                let canon = (solved[orbit[2 * t][0]], solved[orbit[2 * t][1]]);
                if shown != canon {
                    s.wo[oi] |= 1 << q;
                }
            }
        }
        if let Some(mf) = &self.midge_facelets {
            for (j, w) in mf.iter().enumerate() {
                let shown = (state[w[0]], state[w[1]]);
                let t = tmap[shown.0 as usize][shown.1 as usize] as usize;
                s.mt[j] = t as u8;
                let canon = (solved[mf[t][0]], solved[mf[t][1]]);
                if shown != canon {
                    s.mo |= 1 << j;
                }
            }
        }
        s
    }

    fn capply(&self, s: &SN, m: usize) -> SN {
        let mut out = SN {
            cent: [0; 24 * MAX_CORB],
            wt: [0; 24 * MAX_WORB],
            wo: [0; MAX_WORB],
            mt: [0; 12],
            mo: 0,
            mid: [0, 1, 2, 3, 4, 5],
        };
        {
            let mm = &self.midmove[m];
            for f in 0..6 {
                out.mid[mm[f] as usize] = s.mid[f];
            }
        }
        for oi in 0..self.center_orbits.len() {
            let cm = &self.cmove[oi][m];
            for i in 0..24 {
                out.cent[oi * 24 + cm[i] as usize] = s.cent[oi * 24 + i];
            }
        }
        for oi in 0..self.wing_orbits.len() {
            let wm = &self.wmove[oi][m];
            let wf = self.wflip[oi][m];
            for q in 0..24 {
                let q2 = wm[q] as usize;
                out.wt[oi * 24 + q2] = s.wt[oi * 24 + q];
                let bit = ((s.wo[oi] >> q) & 1) ^ ((wf >> q) & 1);
                out.wo[oi] |= bit << q2;
            }
        }
        if self.midge_facelets.is_some() {
            let mm = &self.mmove[m];
            for j in 0..12 {
                let (j2, fl) = mm[j];
                out.mt[j2 as usize] = s.mt[j];
                let bit = (((s.mo >> j) & 1) as u8) ^ fl;
                out.mo |= (bit as u16) << j2;
            }
        }
        out
    }
}

type Base3 = Vec<Option<Vec<usize>>>;

/// Formato do cache: uma linha por orbita, `c`/`w` + movimentos separados por
/// espaco (linha vazia = orbita sem 3-ciclo).
fn gravar_cache_3ciclos(
    path: &std::path::Path,
    centros: &Base3,
    asas: &Base3,
) -> std::io::Result<()> {
    let mut txt = String::new();
    for (marca, lista) in [('c', centros), ('w', asas)] {
        for item in lista {
            let seq = item
                .as_ref()
                .map(|v| v.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            txt.push(marca);
            txt.push(' ');
            txt.push_str(&seq);
            txt.push('\n');
        }
    }
    std::fs::write(path, txt)
}

/// Le o cache e SO aceita sequencias que realmente sao 3-ciclos puros na
/// orbita esperada — assim um cache velho degrada em recalculo, nunca em bug.
fn ler_cache_3ciclos(path: &std::path::Path, cn: &CubeN) -> Option<(Base3, Base3)> {
    let txt = std::fs::read_to_string(path).ok()?;
    let mut centros: Base3 = Vec::new();
    let mut asas: Base3 = Vec::new();
    for linha in txt.lines() {
        let (marca, resto) = linha.split_at(linha.char_indices().nth(1)?.0);
        let seq: Option<Vec<usize>> = {
            let v: Vec<usize> = resto.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.is_empty() {
                None
            } else if v.iter().all(|&m| m < cn.n_moves) {
                Some(v)
            } else {
                return None;
            }
        };
        match marca {
            "c" => centros.push(seq),
            "w" => asas.push(seq),
            _ => return None,
        }
    }
    if centros.len() != cn.center_orbits.len() || asas.len() != cn.wing_orbits.len() {
        return None;
    }
    for (oi, item) in centros.iter().enumerate() {
        if let Some(seq) = item {
            if cn.cycle_support(seq, oi).is_none() {
                return None;
            }
            // e tem de ser identidade nas OUTRAS orbitas de centro
            for outra in 0..cn.center_orbits.len() {
                if outra == oi {
                    continue;
                }
                let p = cn.cycle_perm(seq, outra);
                if (0..24).any(|i| p[i] != i as u8) {
                    return None;
                }
            }
        }
    }
    for (oi, item) in asas.iter().enumerate() {
        if let Some(seq) = item {
            if cn.wing_cycle_support(seq, oi).is_none() {
                return None;
            }
        }
    }
    Some((centros, asas))
}

fn subset_rank(sorted: &[u8; 4]) -> usize {
    fn cnk(nn: usize, k: usize) -> usize {
        if k > nn {
            return 0;
        }
        let mut r = 1usize;
        for i in 0..k {
            r = r * (nn - i) / (i + 1);
        }
        r
    }
    cnk(sorted[0] as usize, 1)
        + cnk(sorted[1] as usize, 2)
        + cnk(sorted[2] as usize, 3)
        + cnk(sorted[3] as usize, 4)
}

// ---------------------------------------------------------------------------
// Predicados e heuristicas sobre o estado compacto
// ---------------------------------------------------------------------------

impl CubeN {
    /// Centros prontos = cada peca com a cor do centro do MEIO da sua face
    /// (ver `center_total`: a orientacao do cubo nao e exigida).
    fn c_centers_solved(&self, s: &SN, orbits: usize, faces_of_last: &[u8]) -> bool {
        for oi in 0..orbits {
            for face in 0..6usize {
                for k in 0..4 {
                    if s.cent[oi * 24 + face * 4 + k] != s.mid[face] {
                        return false;
                    }
                }
            }
        }
        if orbits < self.center_orbits.len() {
            for &face in faces_of_last {
                for k in 0..4 {
                    if s.cent[orbits * 24 + face as usize * 4 + k] != s.mid[face as usize] {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn c_center_h(&self, s: &SN, orbits: usize, faces_of_last: &[u8]) -> u8 {
        // um movimento realoca no maximo 4 centros do meio
        let fora = (0..6).filter(|&f| s.mid[f] != f as u8).count();
        let mut h = fora.div_ceil(4) as u8;
        let per_orbit = |oi: usize, faces: &[u8], h: &mut u8| {
            for &face in faces {
                let mut v = [0u8; 4];
                let mut k = 0;
                for i in 0..24 {
                    if s.cent[oi * 24 + i] == face {
                        if k < 4 {
                            v[k] = i as u8;
                        }
                        k += 1;
                    }
                }
                *h = (*h).max(self.center_dist[oi][face as usize][subset_rank(&v)]);
            }
        };
        let all: [u8; 6] = [0, 1, 2, 3, 4, 5];
        for oi in 0..orbits {
            per_orbit(oi, &all, &mut h);
        }
        if orbits < self.center_orbits.len() {
            per_orbit(orbits, faces_of_last, &mut h);
        }
        h
    }

    /// Aresta t do orbit o esta "agrupada" com a referencia?
    ///   impar: mesma casa e mesmo bit que o midge do tipo t;
    ///   par:   orbita 0 = par alinhado em alguma casa; orbita 1 = mesma casa
    ///          e mesmo bit que o par da orbita 0.
    fn grouped(&self, s: &SN, oi: usize, t: usize) -> bool {
        for j in 0..12 {
            let (a, bb) = (2 * j, 2 * j + 1);
            let ta = s.wt[oi * 24 + a] as usize;
            let tb = s.wt[oi * 24 + bb] as usize;
            let oa = (s.wo[oi] >> a) & 1;
            let ob = (s.wo[oi] >> bb) & 1;
            if ta != t || tb != t || oa != ob {
                continue;
            }
            if self.midge_facelets.is_some() {
                if s.mt[j] as usize == t && ((s.mo >> j) & 1) as u32 == oa {
                    return true;
                }
            } else if oi == 0 {
                return true;
            } else {
                let t0a = s.wt[a] as usize;
                let t0b = s.wt[bb] as usize;
                let o0a = (s.wo[0] >> a) & 1;
                let o0b = (s.wo[0] >> bb) & 1;
                if t0a == t && t0b == t && o0a == o0b && o0a == oa {
                    return true;
                }
            }
        }
        false
    }

    /// Quantas arestas estao agrupadas. Uma casa agrupada corresponde a
    /// exatamente um tipo, entao basta varrer as 12 casas (nao 12x12).
    fn grouped_count(&self, s: &SN, oi: usize) -> usize {
        let mut c = 0;
        for j in 0..12 {
            let (a, bb) = (2 * j, 2 * j + 1);
            let ta = s.wt[oi * 24 + a];
            if ta != s.wt[oi * 24 + bb] {
                continue;
            }
            let oa = (s.wo[oi] >> a) & 1;
            if oa != (s.wo[oi] >> bb) & 1 {
                continue;
            }
            let ok = if self.midge_facelets.is_some() {
                s.mt[j] == ta && ((s.mo >> j) & 1) as u32 == oa
            } else if oi == 0 {
                true
            } else {
                s.wt[a] == ta
                    && s.wt[bb] == ta
                    && (s.wo[0] >> a) & 1 == (s.wo[0] >> bb) & 1
                    && (s.wo[0] >> a) & 1 == oa
            };
            if ok {
                c += 1;
            }
        }
        c
    }

    /// Quantos centros estao na face certa, somando todas as orbitas (mais os
    /// centros do meio nos cubos impares). E a medida que a montagem dos
    /// centros faz subir: monotonica, logo sem ciclo constroi-destroi.
    /// A cor de uma face e a do seu centro do MEIO, nao o indice da face.
    ///
    /// Num cubo impar os centros do meio sao presos ao nucleo: so mudam de
    /// lugar girando o cubo inteiro, e o conjunto de movimentos aqui vai ate
    /// (n+1)/2 camadas — nao ha essa rotacao. Exigir `mid[f] == f` pedia uma
    /// orientacao inalcancavel: medido, o 7x7 empacava em 146/150 com as 144
    /// pecas de orbita ja certas e so os meios "fora do lugar". Fisicamente o
    /// cubo estava resolvido, so segurado de outro jeito.
    fn center_total(&self, s: &SN) -> usize {
        let mut c = 0;
        for oi in 0..self.center_orbits.len() {
            for i in 0..24 {
                if s.cent[oi * 24 + i] == s.mid[i / 4] {
                    c += 1;
                }
            }
        }
        c
    }

    fn center_total_max(&self) -> usize {
        24 * self.center_orbits.len()
    }

    /// Varredura direta de comutadores `V · a b a' b' · V'`, com |V| = niveis.
    /// A familia util e pequena (2916 comutadores = ~12k operacoes), entao vale
    /// enumera-la sozinha em vez de deixar uma busca generica tropecar nela.
    fn commutator_scan<G: Fn(&SN) -> bool + Sync>(
        &self,
        cs: &SN,
        goal: &G,
        niveis: usize,
    ) -> Option<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let nucleo = |base: &SN, pre: &[usize]| -> Option<Vec<usize>> {
            for a in 0..self.n_moves {
                let sa = self.capply(base, a);
                for b in 0..self.n_moves {
                    if a / 3 == b / 3 {
                        continue;
                    }
                    let s = self.capply(&sa, b);
                    let s = self.capply(&s, inv(a));
                    let mut s = self.capply(&s, inv(b));
                    for &v in pre.iter().rev() {
                        s = self.capply(&s, inv(v));
                    }
                    if goal(&s) {
                        let mut seq = pre.to_vec();
                        seq.extend([a, b, inv(a), inv(b)]);
                        seq.extend(pre.iter().rev().map(|&v| inv(v)));
                        return Some(seq);
                    }
                }
            }
            None
        };
        if let Some(r) = nucleo(cs, &[]) {
            return Some(r);
        }
        if niveis == 0 {
            return None;
        }
        // conjugacoes em paralelo: o primeiro achado vence
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let workers = n_workers();
        for nv in 1..=niveis {
            let found: Mutex<Option<Vec<usize>>> = Mutex::new(None);
            let stop = AtomicBool::new(false);
            let cursor = AtomicUsize::new(0);
            std::thread::scope(|sc| {
                for _ in 0..workers {
                    let (found, stop, cursor) = (&found, &stop, &cursor);
                    let nucleo = &nucleo;
                    sc.spawn(move || loop {
                        let v1 = cursor.fetch_add(1, Ordering::Relaxed);
                        if v1 >= self.n_moves || stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let s1 = self.capply(cs, v1);
                        let achou = if nv == 1 {
                            nucleo(&s1, &[v1])
                        } else {
                            let mut r = None;
                            for v2 in 0..self.n_moves {
                                if v2 / 3 == v1 / 3 {
                                    continue;
                                }
                                let s2 = self.capply(&s1, v2);
                                r = nucleo(&s2, &[v1, v2]);
                                if r.is_some() {
                                    break;
                                }
                            }
                            r
                        };
                        if let Some(r) = achou {
                            let mut g = found.lock().unwrap();
                            if g.is_none() {
                                *g = Some(r);
                            }
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                    });
                }
            });
            if let Some(r) = found.into_inner().unwrap() {
                return Some(r);
            }
        }
        None
    }

    /// Movimentos laterais (mesma contagem) que mudam a distribuicao por face.
    fn lateral_moves(&self, cs: &SN, total: usize) -> Vec<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let sig0 = self.center_sig(cs);
        let mut out = Vec::new();
        for m1 in 0..self.n_moves {
            let s1 = self.capply(cs, m1);
            if self.center_total(&s1) == total && self.center_sig(&s1) != sig0 {
                out.push(vec![m1]);
            }
            for m2 in 0..self.n_moves {
                if m2 / 3 == m1 / 3 {
                    continue;
                }
                let s2 = self.capply(&s1, m2);
                let s3 = self.capply(&s2, inv(m1));
                if self.center_total(&s3) == total && self.center_sig(&s3) != sig0 {
                    out.push(vec![m1, m2, inv(m1)]);
                }
            }
        }
        out
    }

    /// Olhar de dois passos: anda de lado (sem perder centros) e checa se ali
    /// aparece um comutador que melhora. Fura o muro do fim de jogo, onde
    /// nenhuma melhora existe no estado atual mas existe a um passo de lado.
    fn improve_lookahead(&self, cs: &SN, total: usize) -> Option<Vec<usize>> {
        let goal = |s: &SN| self.center_total(s) > total;
        let laterais = self.lateral_moves(cs, total);
        if laterais.is_empty() {
            return None;
        }
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let workers = n_workers();
        let found: Mutex<Option<Vec<usize>>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let cursor = AtomicUsize::new(0);
        std::thread::scope(|sc| {
            for _ in 0..workers {
                let (found, stop, cursor, laterais) = (&found, &stop, &cursor, &laterais);
                let goal = &goal;
                sc.spawn(move || loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    if i >= laterais.len() || stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut s = *cs;
                    for &m in &laterais[i] {
                        s = self.capply(&s, m);
                    }
                    if let Some(r) = self.commutator_scan(&s, goal, 0) {
                        let mut seq = laterais[i].clone();
                        seq.extend(r);
                        let mut g = found.lock().unwrap();
                        if g.is_none() {
                            *g = Some(seq);
                        }
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                });
            }
        });
        found.into_inner().unwrap()
    }

    /// Escada para subir a medida global dos centros em pelo menos 1.
    /// Medido: no estado tipico que empaca, ha ~6 comutadores simples que
    /// melhoram e a varredura deles custa milissegundos — por isso ela vem
    /// antes de qualquer enumeracao generica.
    /// `teto` limita a busca construtiva de dois saltos. Elevar quando a fase
    /// empaca ja foi tentado e medido: nao compensou (ver o comentario na fase
    /// de centros), entao o valor e fixo.
    fn improve_centers(&self, cs: &SN, total: usize) -> Option<Vec<usize>> {
        let teto = 4000;
        let dbg = debug_level();
        let marca = |nome: &str, t0: std::time::Instant, achou: bool| {
            if dbg >= 1 {
                eprintln!("CDEGRAU {nome} {:.3}s achou={achou}", t0.elapsed().as_secs_f64());
            }
        };
        let goal = |s: &SN| self.center_total(s) > total;
        let h = |_: &SN| 0u8; // qualquer movimento pode melhorar: sem cota util
        let bs = self.wing_bs(false);
        // do mais barato ao mais caro; nenhuma familia foi retirada, apenas
        // reordenada, e as varreduras instantaneas vem antes das genericas
        let t0 = std::time::Instant::now();
        for d in 1..=2usize {
            if let Some(r) = NSearch::run_at(self, cs, &goal, &h, d) {
                marca("curta", t0, true);
                return Some(r);
            }
        }
        marca("curta", t0, false);
        let t1 = std::time::Instant::now();
        for tier in [1usize, 2] {
            if let Some(r) = self.macro_search(cs, &goal, &bs, tier, None) {
                marca("macro12", t1, true);
                return Some(self.trim_tail(cs, r, &goal));
            }
        }
        marca("macro12", t1, false);
        // Medido no 7x7: o 3-ciclo construido resolve 94 de 165 casos em tempo
        // desprezivel, enquanto `macro3` gastava 706s para acertar 10 de 175.
        // Por isso a construcao vem logo depois dos degraus baratos.
        let t5 = std::time::Instant::now();
        if let Some(r) = self.constructive_center_step_teto(cs, teto) {
            marca("3-ciclo construido", t5, true);
            return Some(r);
        }
        marca("3-ciclo construido", t5, false);
        let t2 = std::time::Instant::now();
        if let Some(r) = self.commutator_scan(cs, &goal, 1) {
            marca("comutador1", t2, true);
            return Some(self.trim_tail(cs, r, &goal));
        }
        marca("comutador1", t2, false);
        let t3 = std::time::Instant::now();
        for prof in 1..=4usize {
            if let Some(r) = self.slice_face_macro(cs, &goal, prof) {
                marca("fatia-encaixa", t3, true);
                return Some(self.trim_tail(cs, r, &goal));
            }
        }
        marca("fatia-encaixa", t3, false);
        let t4 = std::time::Instant::now();
        if let Some(r) = self.macro_search(cs, &goal, &bs, 3, None) {
            marca("macro3", t4, true);
            return Some(self.trim_tail(cs, r, &goal));
        }
        marca("macro3", t4, false);
        if let Some(r) = self.improve_lookahead(cs, total) {
            return Some(r);
        }
        if let Some(r) = self.commutator_scan(cs, &goal, 2) {
            return Some(self.trim_tail(cs, r, &goal));
        }
        NSearch::run_at(self, cs, &goal, &h, 3)
    }

    /// Passeio no platô: sequencia curta que MANTEM a medida dos centros e
    /// reorganiza as pecas erradas. Fura casos como o 3-ciclo por rearranjo,
    /// em vez de busca profunda (que custa segundos e nem sempre acha).
    /// Assinatura da DISTRIBUICAO de cores por face. Um giro de face nao a
    /// altera (so embaralha dentro da face), e por isso nao serve de platô:
    /// mantem a contagem e tambem mantem o caso travado.
    fn center_sig(&self, s: &SN) -> u64 {
        let mut h = 0u64;
        for oi in 0..self.center_orbits.len() {
            for face in 0..6 {
                let mut cnt = [0u8; 6];
                for k in 0..4 {
                    cnt[s.cent[oi * 24 + face * 4 + k] as usize] += 1;
                }
                for (c, &q) in cnt.iter().enumerate() {
                    h = h
                        .wrapping_mul(0x100_0000_01b3)
                        .wrapping_add(((oi * 36 + face * 6 + c) as u64) << 3 | q as u64);
                }
            }
        }
        h
    }

    /// Escape de otimo local: prefere andar de lado (mesma contagem), e se nao
    /// houver, aceita perder 1 ou 2 centros — perto do fim quase todo movimento
    /// muda a contagem, e destruir tudo custa muito mais do que ceder um pouco.
    /// `evitar` sao assinaturas ja visitadas (impede voltar ao mesmo caso).
    fn plateau_shuffle(
        &self,
        cs: &SN,
        total: usize,
        k: usize,
        evitar: &[u64],
    ) -> Vec<usize> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        // candidatas agrupadas pela perda: 0, 1 ou 2 centros
        let mut por_perda: [Vec<Vec<usize>>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let classificar = |s: &SN, seq: Vec<usize>, por: &mut [Vec<Vec<usize>>; 3]| {
            let t = self.center_total(s);
            if t > total {
                return; // melhora: nao e caso do platô (a escada acharia)
            }
            let perda = total - t;
            if perda > 2 {
                return;
            }
            let sig = self.center_sig(s);
            if evitar.contains(&sig) {
                return;
            }
            por[perda].push(seq);
        };
        for m1 in 0..self.n_moves {
            let s1 = self.capply(cs, m1);
            classificar(&s1, vec![m1], &mut por_perda);
            for m2 in 0..self.n_moves {
                if m2 / 3 == m1 / 3 {
                    continue;
                }
                let s2 = self.capply(&s1, m2);
                let s3 = self.capply(&s2, inv(m1));
                classificar(&s3, vec![m1, m2, inv(m1)], &mut por_perda);
            }
        }
        for cand in &por_perda {
            if !cand.is_empty() {
                return cand[k % cand.len()].clone();
            }
        }
        // nada aceitavel: quebra centros de proposito
        let sl = 1 + (k % (self.depths - 1));
        let ax = (k / (self.depths - 1)) % 6;
        let s1 = (ax * self.depths + sl) * 3;
        let s2 = (((ax + 1) % 6) * self.depths) * 3;
        vec![s1, s2, inv(s1)]
    }

    /// Corta movimentos finais desnecessarios de uma sequencia que satisfaz o objetivo.
    fn trim_tail<G: Fn(&SN) -> bool>(&self, cs: &SN, mut seq: Vec<usize>, goal: &G) -> Vec<usize> {
        loop {
            if seq.is_empty() {
                return seq;
            }
            let mut s = *cs;
            for &m in &seq[..seq.len() - 1] {
                s = self.capply(&s, m);
            }
            if goal(&s) {
                seq.pop();
            } else {
                return seq;
            }
        }
    }

    /// b para macros de asas: movimentos simples e, opcionalmente, palavras de
    /// dois (fatia + giro) — e o que cobre "fatia, encaixa, desfaz".
    fn wing_bs(&self, duplas: bool) -> Vec<Vec<usize>> {
        let mut bs: Vec<Vec<usize>> = (0..self.n_moves).map(|m| vec![m]).collect();
        if duplas {
            let faces: Vec<usize> = (0..6)
                .flat_map(|f| (0..3).map(move |pw| (f * self.depths) * 3 + pw))
                .collect();
            let fatias: Vec<usize> = (0..6)
                .flat_map(|f| {
                    (1..self.depths).flat_map(move |d| {
                        (0..3).map(move |pw| ((f * self.depths) + d) * 3 + pw)
                    })
                })
                .collect();
            for &s in &fatias {
                for &g in &faces {
                    if s / 3 == g / 3 {
                        continue;
                    }
                    bs.push(vec![s, g]);
                    bs.push(vec![g, s]);
                }
            }
        }
        bs
    }

    /// Ordem de pre-movimentos: giros de face primeiro, depois fatias largas.
    fn premove_order(&self) -> Vec<usize> {
        let mut v = Vec::with_capacity(self.n_moves);
        for f in 0..6 {
            for pw in 0..3 {
                v.push((f * self.depths) * 3 + pw);
            }
        }
        for f in 0..6 {
            for d in 1..self.depths {
                for pw in 0..3 {
                    v.push((f * self.depths + d) * 3 + pw);
                }
            }
        }
        v
    }

    /// Atomos de construcao: movimentos simples e FATIAS PURAS (uma camada
    /// isolada, que neste conjunto e a composicao `dRw · (d-1)Rw'`). As fatias
    /// puras sao o que permite comutadores cirurgicos nas orbitas internas.
    fn atoms(&self) -> Vec<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let mut v: Vec<Vec<usize>> = (0..self.n_moves).map(|m| vec![m]).collect();
        for f in 0..6 {
            for d in 1..self.depths {
                for pw in 0..3 {
                    let largo = (f * self.depths + d) * 3 + pw;
                    let anterior = (f * self.depths + d - 1) * 3;
                    // dRw seguido de (d-1)Rw' isola a camada d
                    v.push(vec![largo, inv(anterior + pw)]);
                }
            }
        }
        v
    }

    /// Procura, para cada orbita de centro, um comutador `[W, b]` que seja
    /// 3-ciclo PURO (mexe 3 pecas daquela orbita e nada mais). Com ele, o fim
    /// de jogo dos centros deixa de ser busca: conjuga-se para o trio desejado.
    fn find_base_3cycles(&self) -> Vec<Option<Vec<usize>>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let base = self.cstate_of(&self.solved());
        let n_orb = self.center_orbits.len();
        let atomos = self.atoms();
        let mut achado: Vec<Option<Vec<usize>>> = vec![None; n_orb];

        let aplicar = |s: &SN, w: &[usize]| {
            let mut o = *s;
            for &m in w {
                o = self.capply(&o, m);
            }
            o
        };
        let invw = |w: &[usize]| -> Vec<usize> { w.iter().rev().map(|&m| inv(m)).collect() };

        // W = 1 a 3 atomos (cobre "U Rw U'" e as fatias puras); b = 1 atomo
        let encadeia = |a: &Vec<usize>, b: &Vec<usize>| -> Option<Vec<usize>> {
            if a.last().is_some_and(|&l| b.first().is_some_and(|&f| l / 3 == f / 3)) {
                return None;
            }
            let mut w = a.clone();
            w.extend(b.iter().copied());
            Some(w)
        };
        let mut nivel2: Vec<Vec<usize>> = Vec::new();
        for a1 in &atomos {
            for a2 in &atomos {
                if let Some(w) = encadeia(a1, a2) {
                    nivel2.push(w);
                }
            }
        }

        let resultado: Mutex<Vec<Option<Vec<usize>>>> = Mutex::new(achado.clone());
        let testa = |w: &Vec<usize>, res: &Mutex<Vec<Option<Vec<usize>>>>| {
            let sw = aplicar(&base, w);
            let wi = invw(w);
            for b in &atomos {
                let s = aplicar(&sw, b);
                let s = aplicar(&s, &wi);
                let s = aplicar(&s, &invw(b));
                if s.mid != base.mid {
                    continue;
                }
                // suporte pela PERMUTACAO de casas: duas pecas de mesma cor
                // podem trocar sem mudar cor nenhuma, e isso nao e 3-ciclo
                let mut seq = w.clone();
                seq.extend(b.iter().copied());
                seq.extend(wi.iter().copied());
                seq.extend(invw(b));
                let mut mexidas = vec![0usize; n_orb];
                for oi in 0..n_orb {
                    let p = self.cycle_perm(&seq, oi);
                    mexidas[oi] = (0..24).filter(|&i| p[i] != i as u8).count();
                }
                if mexidas.iter().sum::<usize>() != 3 {
                    continue;
                }
                let oi = mexidas.iter().position(|&x| x == 3).unwrap();
                let mut g = res.lock().unwrap();
                if g[oi].is_none() {
                    g[oi] = Some(seq);
                }
            }
        };

        // niveis 1 e 2 sao baratos: sequencial
        for w in atomos.iter().chain(nivel2.iter()) {
            testa(w, &resultado);
        }
        achado = resultado.lock().unwrap().clone();
        if achado.iter().all(|x| x.is_some()) {
            return achado;
        }

        // nivel 3 em paralelo, fatiado pelo primeiro atomo
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let cursor = AtomicUsize::new(0);
        let pronto = AtomicBool::new(false);
        let workers = n_workers();
        std::thread::scope(|sc| {
            for _ in 0..workers {
                let (cursor, pronto, resultado) = (&cursor, &pronto, &resultado);
                let (atomos, nivel2, testa) = (&atomos, &nivel2, &testa);
                sc.spawn(move || loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    if i >= atomos.len() || pronto.load(Ordering::Relaxed) {
                        break;
                    }
                    for w2 in nivel2.iter() {
                        if let Some(w) = encadeia(&atomos[i], w2) {
                            testa(&w, resultado);
                        }
                    }
                    if resultado.lock().unwrap().iter().all(|x| x.is_some()) {
                        pronto.store(true, Ordering::Relaxed);
                        break;
                    }
                });
            }
        });
        let saida = resultado.lock().unwrap().clone();
        saida
    }

    /// Permutacao de casas que uma sequencia causa na orbita de centro `oi`:
    /// `p[i]` = para onde vai o conteudo da casa `i`.
    fn cycle_perm(&self, seq: &[usize], oi: usize) -> [u8; 24] {
        let mut p: [u8; 24] = std::array::from_fn(|i| i as u8);
        for &m in seq {
            let cm = &self.cmove[oi][m];
            for x in p.iter_mut() {
                *x = cm[*x as usize];
            }
        }
        p
    }

    /// As 3 casas que o 3-ciclo base move, na ordem do ciclo (x -> y -> z -> x).
    fn cycle_support(&self, seq: &[usize], oi: usize) -> Option<[u8; 3]> {
        let p = self.cycle_perm(seq, oi);
        let movidas: Vec<u8> =
            (0..24u8).filter(|&i| p[i as usize] != i).collect();
        if movidas.len() != 3 {
            return None;
        }
        let x = movidas[0];
        let y = p[x as usize];
        let z = p[y as usize];
        if p[z as usize] != x {
            return None;
        }
        Some([x, y, z])
    }

    /// Arvore de conjugacao: de qual trio de casas se chega a qual, e por qual
    /// movimento. Permite levar o trio base a QUALQUER trio, instantaneamente.
    /// Indice do trio (a,b,c) = a*576 + b*24 + c.
    /// Variantes de um 3-ciclo puro que saem DE GRACA: o inverso e as
    /// "rotacoes" — remapear as faces da sequencia leva o ciclo a outro trio
    /// com o MESMO comprimento. Nao assumimos convencao nenhuma: todas as 720
    /// permutacoes de faces (com e sem espelhar o sentido) sao testadas por
    /// simulacao, e so passa o que continua sendo um 3-ciclo puro da mesma
    /// orbita. Cada variante vira uma raiz a mais na arvore de conjugacao —
    /// e cada nivel a menos na arvore economiza 2 movimentos por ciclo.
    fn variantes_do_ciclo(
        &self,
        seq: &[usize],
        centros: bool,
        oi: usize,
    ) -> Vec<([u8; 3], Vec<usize>)> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let inverso: Vec<usize> = seq.iter().rev().map(|&m| inv(m)).collect();
        let base = self.cstate_of(&self.solved());
        let n_corb = self.center_orbits.len();
        let n_worb = self.wing_orbits.len();
        let mut saida: Vec<([u8; 3], Vec<usize>)> = Vec::new();
        // permutacoes de 6 faces por troca de pares (geracao simples)
        let mut perms: Vec<[usize; 6]> = vec![[0, 1, 2, 3, 4, 5]];
        for k in 1..6 {
            let mut prox = Vec::new();
            for p in &perms {
                for j in 0..=k {
                    let mut q = *p;
                    q.swap(j, k);
                    prox.push(q);
                }
            }
            perms = prox;
        }
        for origem in [seq, inverso.as_slice()] {
            for p in &perms {
                for espelha in [false, true] {
                    let cand: Vec<usize> = origem
                        .iter()
                        .map(|&m| {
                            let f = m / 3 / self.depths;
                            let d = m / 3 % self.depths;
                            let pw = if espelha { 2 - m % 3 } else { m % 3 };
                            (p[f] * self.depths + d) * 3 + pw
                        })
                        .collect();
                    // Pureza por SIMULACAO, com os mesmos criterios do build
                    // (permutacao de casas, nao cores — pecas iguais trocadas
                    // nao aparecem na cor):
                    //   centros: meios intactos, 3 casas mexidas em oi, zero
                    //     nas outras orbitas de centro;
                    //   asas: centros/meios/mt/mo intactos, nenhuma asa
                    //     virada, 3 casas mexidas em oi, zero nas outras.
                    let mut s = base;
                    for &m in &cand {
                        s = self.capply(&s, m);
                    }
                    if s.mid != base.mid {
                        continue;
                    }
                    let mut ok = true;
                    if centros {
                        for o in 0..n_corb {
                            let p = self.cycle_perm(&cand, o);
                            let mexidas = (0..24).filter(|&i| p[i] != i as u8).count();
                            if (o == oi && mexidas != 3) || (o != oi && mexidas != 0) {
                                ok = false;
                                break;
                            }
                        }
                    } else {
                        if s.cent != base.cent
                            || s.mt != base.mt
                            || s.mo != base.mo
                            || s.wo.iter().any(|&x| x != 0)
                        {
                            continue;
                        }
                        for o in 0..n_worb {
                            let mut p: [u8; 24] = std::array::from_fn(|i| i as u8);
                            for &m in &cand {
                                let wm = &self.wmove[o][m];
                                for x in p.iter_mut() {
                                    *x = wm[*x as usize];
                                }
                            }
                            let mexidas = (0..24).filter(|&i| p[i] != i as u8).count();
                            if (o == oi && mexidas != 3) || (o != oi && mexidas != 0) {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        continue;
                    }
                    let trio = if centros {
                        self.cycle_support(&cand, oi)
                    } else {
                        self.wing_cycle_support(&cand, oi)
                    };
                    let Some(trio) = trio else { continue };
                    if !saida.iter().any(|(t, _)| *t == trio) {
                        saida.push((trio, cand));
                    }
                }
            }
        }
        saida
    }

    /// Arvore de conjugacao MULTI-RAIZ: BFS a partir de todos os trios-base ao
    /// mesmo tempo. Cada casa guarda (pai, movimento, fonte); a fonte diz qual
    /// dos ciclos-base usar ao chegar na raiz. MEDIDO antes: |V| medio de 3.8
    /// com raiz unica — cada nivel economizado sao 2 movimentos por ciclo.
    fn triple_tree_multi(
        &self,
        oi: usize,
        raizes: &[([u8; 3], Vec<usize>)],
        asas: bool,
    ) -> Vec<(u16, u8, u8)> {
        let idx = |t: [u8; 3]| t[0] as usize * 576 + t[1] as usize * 24 + t[2] as usize;
        let mut arvore = vec![(u16::MAX, u8::MAX, 0u8); 24 * 24 * 24];
        let mut fila = std::collections::VecDeque::new();
        for (fonte, (trio, _)) in raizes.iter().enumerate() {
            let i = idx(*trio);
            if arvore[i].1 == u8::MAX {
                arvore[i] = (u16::MAX, u8::MAX - 1, fonte as u8);
                fila.push_back(*trio);
            }
        }
        while let Some(t) = fila.pop_front() {
            let de = idx(t);
            let fonte = arvore[de].2;
            for m in 0..self.n_moves {
                let tab = if asas { &self.wmove[oi][m] } else { &self.cmove[oi][m] };
                let nt = [tab[t[0] as usize], tab[t[1] as usize], tab[t[2] as usize]];
                let para = idx(nt);
                if arvore[para].1 == u8::MAX {
                    arvore[para] = (de as u16, m as u8, fonte);
                    fila.push_back(nt);
                }
            }
        }
        arvore
    }

    /// Procura, para cada orbita de ASAS, um comutador que 3-cicle exatamente
    /// tres casas de asa sem tocar em mais nada (centros, meios, outras asas,
    /// nem virar asa alguma). Mesma ideia dos centros: com ele, o fim de jogo
    /// do agrupamento passa a ser construido, nao procurado.
    fn find_base_wing_3cycles(&self) -> Vec<Option<Vec<usize>>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let base = self.cstate_of(&self.solved());
        let n_worb = self.wing_orbits.len();
        let atomos = self.atoms();
        let aplicar = |s: &SN, w: &[usize]| {
            let mut o = *s;
            for &m in w {
                o = self.capply(&o, m);
            }
            o
        };
        let invw = |w: &[usize]| -> Vec<usize> { w.iter().rev().map(|&m| inv(m)).collect() };

        // Suporte pela PERMUTACAO de casas de asa, exigindo: nada virado
        // (wo == 0), centros e meios intactos, e uma unica orbita mexida.
        let suporte = |s: &SN, seq: &[usize]| -> Option<(usize, usize)> {
            if s.cent != base.cent || s.mid != base.mid || s.mt != base.mt || s.mo != base.mo {
                return None;
            }
            if s.wo.iter().any(|&x| x != 0) {
                return None; // virou alguma asa: nao serve
            }
            let mut qual = usize::MAX;
            let mut total = 0;
            for oi in 0..n_worb {
                let mut p: [u8; 24] = std::array::from_fn(|i| i as u8);
                for &m in seq {
                    let wm = &self.wmove[oi][m];
                    for x in p.iter_mut() {
                        *x = wm[*x as usize];
                    }
                }
                let mexidas = (0..24).filter(|&i| p[i] != i as u8).count();
                if mexidas > 0 {
                    if qual != usize::MAX {
                        return None; // mexeu em duas orbitas
                    }
                    qual = oi;
                    total = mexidas;
                }
            }
            if qual == usize::MAX {
                None
            } else {
                Some((qual, total))
            }
        };

        let mut achado: Vec<Option<Vec<usize>>> = vec![None; n_worb];
        let mut palavras: Vec<Vec<usize>> = atomos.clone();
        for a1 in &atomos {
            for a2 in &atomos {
                if a1.last().is_some_and(|&l| a2.first().is_some_and(|&f| l / 3 == f / 3)) {
                    continue;
                }
                let mut w = a1.clone();
                w.extend(a2.iter().copied());
                palavras.push(w);
            }
        }
        let nivel2 = palavras.clone();
        let resultado: Mutex<Vec<Option<Vec<usize>>>> = Mutex::new(achado.clone());
        let testa = |w: &Vec<usize>, res: &Mutex<Vec<Option<Vec<usize>>>>| {
            let sw = aplicar(&base, w);
            let wi = invw(w);
            for b in &atomos {
                let s = aplicar(&sw, b);
                let s = aplicar(&s, &wi);
                let s = aplicar(&s, &invw(b));
                let mut seq = w.clone();
                seq.extend(b.iter().copied());
                seq.extend(wi.iter().copied());
                seq.extend(invw(b));
                if let Some((oi, 3)) = suporte(&s, &seq) {
                    let mut g = res.lock().unwrap();
                    if g[oi].is_none() {
                        g[oi] = Some(seq);
                    }
                }
            }
        };
        for w in &palavras {
            testa(w, &resultado);
        }
        achado = resultado.lock().unwrap().clone();
        if achado.iter().all(|x| x.is_some()) {
            return achado;
        }
        // nivel 3 em paralelo
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let cursor = AtomicUsize::new(0);
        let pronto = AtomicBool::new(false);
        let workers = n_workers();
        std::thread::scope(|sc| {
            for _ in 0..workers {
                let (cursor, pronto, resultado) = (&cursor, &pronto, &resultado);
                let (atomos, nivel2, testa) = (&atomos, &nivel2, &testa);
                sc.spawn(move || loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    if i >= atomos.len() || pronto.load(Ordering::Relaxed) {
                        break;
                    }
                    for w2 in nivel2.iter() {
                        if atomos[i]
                            .last()
                            .is_some_and(|&l| w2.first().is_some_and(|&f| l / 3 == f / 3))
                        {
                            continue;
                        }
                        let mut w = atomos[i].clone();
                        w.extend(w2.iter().copied());
                        testa(&w, resultado);
                    }
                    if resultado.lock().unwrap().iter().all(|x| x.is_some()) {
                        pronto.store(true, Ordering::Relaxed);
                        break;
                    }
                });
            }
        });
        let saida = resultado.lock().unwrap().clone();
        saida
    }

    /// Sequencia que 3-cicla exatamente as casas `alvo` da orbita `oi`:
    /// conjuga o 3-ciclo base (`V · C · V'`), com V vindo da arvore de trios.
    /// Como conjugacao preserva o tamanho do suporte, o resultado tambem e
    /// cirurgico: mexe so essas 3 pecas.
    fn cycle_triple(&self, oi: usize, alvo: [u8; 3]) -> Option<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let arvore = self.triple_trees.get(oi)?;
        if arvore.is_empty() {
            return None;
        }
        let idx = |t: [u8; 3]| t[0] as usize * 576 + t[1] as usize * 24 + t[2] as usize;
        // caminho da raiz mais proxima (fonte) ate o alvo
        let mut caminho = Vec::new();
        let mut atual = idx(alvo);
        loop {
            let (pai, mov, _) = arvore[atual];
            if mov == u8::MAX {
                return None; // trio inalcancavel
            }
            if mov == u8::MAX - 1 {
                break; // raiz
            }
            caminho.push(mov as usize);
            atual = pai as usize;
        }
        let base = &self.base3_fontes[oi].get(arvore[atual].2 as usize)?.1;
        caminho.reverse(); // W: leva o trio da fonte ao alvo
        let mut seq: Vec<usize> = caminho.iter().rev().map(|&m| inv(m)).collect();
        seq.extend(base.iter().copied());
        seq.extend(caminho.iter().copied());
        Some(seq)
    }

    /// Sinal da permutacao que um movimento causa nas casas de asa da orbita:
    /// true = IMPAR. Um par de asas trocado (o que parece "aresta virada") e
    /// uma transposicao, logo so uma sequencia impar consegue desfazer.
    #[cfg(test)]
    fn wing_move_is_odd(&self, oi: usize, m: usize) -> bool {
        let wm = &self.wmove[oi][m];
        let mut visto = [false; 24];
        let mut ciclos = 0;
        for i in 0..24 {
            if visto[i] {
                continue;
            }
            ciclos += 1;
            let mut j = i;
            while !visto[j] {
                visto[j] = true;
                j = wm[j] as usize;
            }
        }
        (24 - ciclos) % 2 == 1
    }

    /// Movimentos de sinal impar nessa orbita de asas. Usado pelo teste
    /// `movimentos_impares_das_asas`, que registra quais sao (medido: os giros
    /// largos; a fatia pura e par, e por isso nao corrige paridade).
    #[cfg(test)]
    fn wing_odd_moves(&self, oi: usize) -> Vec<usize> {
        (0..self.n_moves).filter(|&m| self.wing_move_is_odd(oi, m)).collect()
    }

    fn perm_sign_odd(p: &[u8]) -> bool {
        let n = p.len();
        let mut visto = vec![false; n];
        let mut ciclos = 0;
        for i in 0..n {
            if visto[i] {
                continue;
            }
            ciclos += 1;
            let mut j = i;
            while !visto[j] {
                visto[j] = true;
                j = p[j] as usize;
            }
        }
        (n - ciclos) % 2 == 1
    }

    /// Sinal da permutacao das asas no ESTADO (nao de uma sequencia): as duas
    /// asas de um tipo sao distinguiveis pelo bit, entao o estado e uma
    /// permutacao de 24 pecas. E esse sinal que decide se o agrupamento vai
    /// esbarrar em paridade — saber disso ANTES evita montar tudo e refazer.
    fn wing_state_sign_odd(&self, s: &SN, oi: usize) -> bool {
        let mut p = [0u8; 24];
        for q in 0..24 {
            let bit = ((s.wo[oi] >> q) & 1) as u8;
            p[q] = s.wt[oi * 24 + q] * 2 + bit;
        }
        Self::perm_sign_odd(&p)
    }

    /// Sinais de uma sequencia: por orbita de asas e nas arestas do meio.
    fn seq_signs(&self, seq: &[usize]) -> (Vec<bool>, bool) {
        let mut asas = Vec::with_capacity(self.wing_orbits.len());
        for oi in 0..self.wing_orbits.len() {
            let mut p: [u8; 24] = std::array::from_fn(|i| i as u8);
            for &m in seq {
                let wm = &self.wmove[oi][m];
                for x in p.iter_mut() {
                    *x = wm[*x as usize];
                }
            }
            asas.push(Self::perm_sign_odd(&p));
        }
        let mut meio = false;
        if self.midge_facelets.is_some() {
            let mut p: [u8; 12] = std::array::from_fn(|i| i as u8);
            for &m in seq {
                let mm = &self.mmove[m];
                for x in p.iter_mut() {
                    *x = mm[*x as usize].0;
                }
            }
            meio = Self::perm_sign_odd(&p);
        }
        (asas, meio)
    }

    /// Sequencia curta que troca a paridade SO da orbita `oi` (impar nela, par
    /// nas outras e nas arestas do meio). E o que corrige o par de asas
    /// trocado: o que importa e a paridade RELATIVA a referencia, e um giro
    /// largo isolado costuma inverter duas orbitas ao mesmo tempo, sem efeito.
    /// Varias candidatas, para poder variar entre tentativas. O criterio e ser
    /// impar na orbita `oi` e par nas OUTRAS orbitas de asas — as arestas do
    /// meio ficam livres, porque nos cubos impares a correcao valida (um giro
    /// largo) e impar nelas tambem, e exigir paridade par ali eliminava a
    /// unica saida.
    fn wing_parity_fixes(&self, oi: usize) -> Vec<Vec<usize>> {
        // Primeiro as "cirurgicas" (impar so nessa orbita) e DEPOIS as que
        // tambem mexem nas outras: exigir sempre pureza ja tinha me custado a
        // unica saida no caso das arestas do meio, e o mesmo vale entre
        // orbitas — ha cubo 6x6 que so fecha com uma correcao nao pura.
        let puro = |seq: &[usize]| {
            let (asas, _meio) = self.seq_signs(seq);
            asas[oi] && asas.iter().enumerate().all(|(k, &s)| k == oi || !s)
        };
        let qualquer = |seq: &[usize]| self.seq_signs(seq).0[oi];
        let mut saida = Vec::new();
        let mut soltas = Vec::new();
        for m in 0..self.n_moves {
            if puro(&[m]) {
                saida.push(vec![m]);
            } else if qualquer(&[m]) {
                soltas.push(vec![m]);
            }
        }
        let serve = puro;
        if saida.len() < 4 {
            for m1 in 0..self.n_moves {
                for m2 in 0..self.n_moves {
                    if m1 / 3 == m2 / 3 {
                        continue;
                    }
                    if serve(&[m1, m2]) {
                        saida.push(vec![m1, m2]);
                        if saida.len() >= 8 {
                            saida.extend(soltas);
                            return saida;
                        }
                    }
                }
            }
        }
        saida.extend(soltas); // as nao puras entram no fim da fila
        saida
    }

    /// Suporte (as 3 casas movidas, em ordem de ciclo) de um 3-ciclo de asas.
    fn wing_cycle_support(&self, seq: &[usize], oi: usize) -> Option<[u8; 3]> {
        let mut p: [u8; 24] = std::array::from_fn(|i| i as u8);
        for &m in seq {
            let wm = &self.wmove[oi][m];
            for x in p.iter_mut() {
                *x = wm[*x as usize];
            }
        }
        let movidas: Vec<u8> = (0..24u8).filter(|&i| p[i as usize] != i).collect();
        if movidas.len() != 3 {
            return None;
        }
        let x = movidas[0];
        let y = p[x as usize];
        let z = p[y as usize];
        if p[z as usize] != x {
            return None;
        }
        Some([x, y, z])
    }

    /// Sequencia que 3-cicla as casas `alvo` da orbita de ASAS `oi`.
    fn wing_cycle_triple(&self, oi: usize, alvo: [u8; 3]) -> Option<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let arvore = self.wing_trees.get(oi)?;
        if arvore.is_empty() {
            return None;
        }
        let idx = |t: [u8; 3]| t[0] as usize * 576 + t[1] as usize * 24 + t[2] as usize;
        let mut caminho = Vec::new();
        let mut atual = idx(alvo);
        loop {
            let (pai, mov, _) = arvore[atual];
            if mov == u8::MAX {
                return None;
            }
            if mov == u8::MAX - 1 {
                break;
            }
            caminho.push(mov as usize);
            atual = pai as usize;
        }
        let base = &self.wbase3_fontes[oi].get(arvore[atual].2 as usize)?.1;
        caminho.reverse();
        let mut seq: Vec<usize> = caminho.iter().rev().map(|&m| inv(m)).collect();
        seq.extend(base.iter().copied());
        seq.extend(caminho.iter().copied());
        Some(seq)
    }

    /// Passo CONSTRUTIVO das asas: leva uma asa solta para a casa onde ela
    /// forma par com a referencia, usando 3-ciclos cirurgicos. Testa varias
    /// terceiras casas porque o caminho define o bit de orientacao da asa.
    /// Quantos pares agrupados estao "invertidos" (bit 1). Medido: o 3x3 so
    /// aceita a reducao quando esse numero e PAR — com um numero impar ele
    /// acusa "uma aresta esta invertida". Num cubo par a orbita 0 nao tem peca
    /// de referencia, entao essa orientacao e escolha nossa, e da para acertar
    /// ao fechar o ultimo par em vez de refazer tudo depois.
    fn invertidos(&self, s: &SN, oi: usize) -> usize {
        (0..12).filter(|&j| (s.wo[oi] >> (2 * j)) & 1 == 1).count()
    }

    fn constructive_wing_step(&self, cs: &SN, oi: usize) -> Option<Vec<usize>> {
        self.constructive_wing_step_par(cs, oi, false)
    }


    /// `exigir_par`: so aceita a sequencia se, ao fechar as 12 arestas, o
    /// numero de pares invertidos ficar par (senao o 3x3 recusa a reducao).
    fn constructive_wing_step_par(
        &self,
        cs: &SN,
        oi: usize,
        exigir_par: bool,
    ) -> Option<Vec<usize>> {
        let count = self.grouped_count(cs, oi);
        let soltos: Vec<usize> = (0..12).filter(|&t| !self.grouped(cs, oi, t)).collect();
        // casas "livres": pertencem a pares ainda nao formados, logo podem
        // receber peca sem estragar nada
        let livres: Vec<usize> = (0..24)
            .filter(|&q| !self.grouped(cs, oi, cs.wt[oi * 24 + q] as usize))
            .collect();
        let aplicar = |s: &SN, seq: &[usize]| {
            let mut o = *s;
            for &m in seq {
                o = self.capply(&o, m);
            }
            o
        };
        // Referencia de cada casa (quando existe): nos impares, a aresta do
        // meio; nos pares com orbita > 0, o par ja formado da orbita 0. Na
        // orbita 0 dos pares nao ha referencia e a conta fica so nos pares.
        let ref_da_casa = |s: &SN, j: usize| -> Option<u8> {
            if self.midge_facelets.is_some() {
                Some(s.mt[j])
            } else if oi > 0 {
                (s.wt[2 * j] == s.wt[2 * j + 1]).then(|| s.wt[2 * j])
            } else {
                None
            }
        };
        // Asas ja na casa certa: e o que um ciclo em CADEIA melhora alem do
        // par fechado — a segunda asa movida tambem chega em casa.
        let certas = |s: &SN| -> usize {
            let mut c = 0;
            for j in 0..12 {
                if let Some(t) = ref_da_casa(s, j) {
                    for q in [2 * j, 2 * j + 1] {
                        if s.wt[oi * 24 + q] == t {
                            c += 1;
                        }
                    }
                }
            }
            c
        };
        // um 3-ciclo por vez nao fecha um par (sao DUAS asas): tenta um e,
        // se preciso, um segundo em cima dele.
        //
        // O terceiro vertice `x` do trio recebe o conteudo que estava em
        // `destino` — entao a casa-LAR desse conteudo vem primeiro na fila: se
        // servir, o mesmo ciclo arruma DUAS asas (a cadeia dos centros,
        // aplicada as asas). Motivo: o raio-X mostrou as arestas dominando o
        // comprimento nos tres tamanhos (54 a 73%), a ~30 movimentos por
        // aresta, e cada 3-ciclo destes custa 8 a 16.
        let tenta_um = |base: &SN, origem: usize, destino: usize| -> Vec<Vec<usize>> {
            let desloc = base.wt[oi * 24 + destino];
            let mut fila: Vec<usize> = Vec::new();
            for j in 0..12 {
                if ref_da_casa(base, j) == Some(desloc) {
                    for q in [2 * j, 2 * j + 1] {
                        if livres.contains(&q) && base.wt[oi * 24 + q] != desloc {
                            fila.push(q);
                        }
                    }
                }
            }
            let resto: Vec<usize> =
                livres.iter().copied().filter(|q| !fila.contains(q)).collect();
            fila.extend(resto);
            fila.push(0);
            let mut saida = Vec::new();
            for &x in &fila {
                if x == origem || x == destino {
                    continue;
                }
                for trio in [
                    [origem as u8, destino as u8, x as u8],
                    [origem as u8, x as u8, destino as u8],
                ] {
                    if let Some(seq) = self.wing_cycle_triple(oi, trio) {
                        let s = aplicar(base, &seq);
                        if s.wt[oi * 24 + destino] as usize
                            == base.wt[oi * 24 + origem] as usize
                        {
                            saida.push(seq);
                        }
                    }
                }
            }
            saida
        };

        // Pool de candidatos ACEITOS atraves de todos os alvos, com teto — a
        // mesma receita medida nos centros. Antes, o primeiro (origem, destino)
        // que aceitava ganhava, e a media ficava em 1.19 pares por chamada
        // (WGASTO: 19.2 mov/par, 100% desta funcao); comparar alvos diferentes
        // e o que deixa a cadeia (2 pares num ciclo) ser escolhida.
        let mut pool: Vec<(usize, usize, usize, Vec<usize>)> = Vec::new();
        'alvos: for &t in &soltos {
            let (a, b) = self.wing_positions(cs, oi, t);
            if a == usize::MAX || b == usize::MAX {
                continue;
            }
            for j in 0..12usize {
                // Casa-destino valida para o tipo t:
                //   impar: onde a aresta do meio e do tipo t;
                //   par, orbita >0: onde a orbita 0 tem par do tipo t;
                //   par, orbita 0: qualquer casa ainda sem par (nao ha
                //     referencia — agrupar e so juntar as duas asas).
                let serve = if self.midge_facelets.is_some() {
                    cs.mt[j] as usize == t
                } else if oi > 0 {
                    cs.wt[2 * j] == cs.wt[2 * j + 1] && cs.wt[2 * j] as usize == t
                } else {
                    cs.wt[oi * 24 + 2 * j] != cs.wt[oi * 24 + 2 * j + 1]
                };
                if !serve {
                    continue;
                }
                for (da, db) in [(2 * j, 2 * j + 1), (2 * j + 1, 2 * j)] {
                    for (origem, parceira) in [(a, b), (b, a)] {
                        // caso facil: um 3-ciclo ja fecha
                        // aceita se agrupou mais uma; fechando as 12, a
                        // orientacao tem de deixar o total de invertidos par
                        let aceita = |s: &SN| {
                            let c = self.grouped_count(s, oi);
                            // a orientacao que importa e a da orbita 0: e dela
                            // que o mapa 3x3 le as arestas
                            c > count && (!exigir_par || c < 12 || self.invertidos(s, 0) % 2 == 0)
                        };
                        // Todo ciclo aceito entra no pool com sua pontuacao
                        // (pares, asas na casa certa, mais curto); a escolha e
                        // feita uma vez, sobre alvos DIFERENTES.
                        let candidatos = tenta_um(cs, origem, da);
                        let mut algum = false;
                        for seq in &candidatos {
                            let s1 = aplicar(cs, seq);
                            if aceita(&s1) {
                                algum = true;
                                pool.push((
                                    self.grouped_count(&s1, oi),
                                    certas(&s1),
                                    usize::MAX - seq.len(),
                                    seq.clone(),
                                ));
                                if pool.len() >= 16 {
                                    break 'alvos;
                                }
                            }
                        }
                        if algum {
                            continue;
                        }
                        for seq in &candidatos {
                            let s1 = aplicar(cs, seq);
                            // um segundo 3-ciclo para a parceira
                            let (p1, p2) = self.wing_positions(&s1, oi, t);
                            let resto = if p1 == da { p2 } else { p1 };
                            if resto == usize::MAX || resto == db {
                                continue;
                            }
                            for seq2 in tenta_um(&s1, resto, db) {
                                let s2 = aplicar(&s1, &seq2);
                                if aceita(&s2) {
                                    let mut junta = seq.clone();
                                    junta.extend(seq2);
                                    pool.push((
                                        self.grouped_count(&s2, oi),
                                        certas(&s2),
                                        usize::MAX - junta.len(),
                                        junta,
                                    ));
                                    if pool.len() >= 16 {
                                        break 'alvos;
                                    }
                                }
                            }
                        }
                        let _ = parceira;
                    }
                }
            }
        }
        pool.into_iter().max_by_key(|c| (c.0, c.1, c.2)).map(|c| c.3)
    }

    /// Passo CONSTRUTIVO dos centros: acha tres casas erradas em cadeia e as
    /// arruma com um 3-ciclo cirurgico. Sempre progride (ao menos +1), logo
    /// nao ha platô nem ciclo — e o que garante o fechamento dos centros.
    /// `teto` limita as combinacoes avaliadas na busca de dois saltos: a
    /// varredura completa sao ~331 mil combinacoes.
    fn constructive_center_step_teto(&self, cs: &SN, teto: usize) -> Option<Vec<usize>> {
        let total = self.center_total(cs);
        let mut melhor: Option<Vec<usize>> = None;
        let mut cand: Vec<(usize, usize, Vec<usize>)> = Vec::new(); // (pecas, -len, seq)
        'orbitas: for oi in 0..self.center_orbits.len() {
            if self.base3[oi].is_none() {
                continue;
            }
            // A cor certa de uma casa e a do centro do MEIO da face dela (o
            // cubo pode estar numa orientacao diferente da canonica).
            let cor = |i: usize| cs.cent[oi * 24 + i];
            let cor_da_face = |f: usize| cs.mid[f];
            let face_da_cor = |c: u8| (0..6).find(|&f| cs.mid[f] == c).unwrap_or(0);
            let erradas: Vec<usize> =
                (0..24).filter(|&i| cor(i) != cor_da_face(i / 4)).collect();
            if erradas.len() < 2 {
                continue;
            }
            // MEDIDO (CGASTO no 7x7): este degrau produz 69% dos movimentos dos
            // centros, a 7.9 mov/peca — e cada ciclo custa ~18 movimentos,
            // dominados pela conjugacao. Entre os trios que melhoram, vale
            // escolher o de sequencia mais curta; mas escolher o "melhor"
            // varrendo tudo ja foi medido (6.6s -> 6 min), entao ha TETO: no
            // maximo 16 candidatos que melhoram, e para.
            'busca: for &p in &erradas {
                let destino_p = face_da_cor(cor(p)); // face onde a peca de p pertence
                // q: casa errada na face de destino de p
                for &q in erradas.iter().filter(|&&q| q != p && q / 4 == destino_p) {
                    let destino_q = face_da_cor(cor(q));
                    // r: casa na face de destino de q. Ordem = cadeia FECHADA
                    // primeiro (o conteudo de r pertence a face de p: o mesmo
                    // ciclo arruma 3 pecas), depois as erradas (+2), depois o
                    // resto. So a ORDEM muda — o aceite continua sendo o
                    // primeiro que melhora, porque escolher o "melhor" global
                    // ja foi medido e custou minutos (ver comentario abaixo).
                    let fecha_ciclo = |r: usize| face_da_cor(cor(r)) == p / 4;
                    let candidatos_r = erradas
                        .iter()
                        .copied()
                        .filter(|&r| r != p && r != q && r / 4 == destino_q && fecha_ciclo(r))
                        .chain(erradas.iter().copied().filter(|&r| {
                            r != p && r != q && r / 4 == destino_q && !fecha_ciclo(r)
                        }))
                        .chain(
                            (0..24).filter(|&r| r != p && r != q && r / 4 == destino_q),
                        );
                    for r in candidatos_r {
                        // tenta as duas orientacoes do ciclo
                        for trio in [[p as u8, q as u8, r as u8], [p as u8, r as u8, q as u8]] {
                            // Aceita o PRIMEIRO que melhora. Tentei escolher o
                            // de melhor custo por peca ganha, para encurtar a
                            // solucao: o 7x7 passou de 6.6s para mais de 6
                            // minutos e o ganho de comprimento nao se
                            // confirmou. Medido e revertido.
                            if let Some(seq) = self.cycle_triple(oi, trio) {
                                let mut s = *cs;
                                for &m in &seq {
                                    s = self.capply(&s, m);
                                }
                                let t = self.center_total(&s);
                                if t > total {
                                    cand.push((t - total, usize::MAX - seq.len(), seq));
                                    if cand.len() >= 16 {
                                        break 'busca;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !cand.is_empty() {
                break 'orbitas;
            }
        }
        if let Some((_, _, seq)) = cand.into_iter().max_by_key(|c| (c.0, c.1)) {
            melhor = Some(seq);
        }
        if melhor.is_some() {
            return melhor;
        }
        // Nenhum trio direto serviu: o grupo nao alcanca todos os trios em
        // algumas orbitas (as oblicuas tem classes separadas). Faz em DOIS
        // saltos — leva a peca a uma casa intermediaria e de la ao destino.
        // Sem isso o solver empacava perto do fim e reiniciava tudo.
        // O teto existe porque a varredura completa sao ~331 mil combinacoes:
        // sem ele a suite saiu de 114s para 1461s.
        let mut avaliadas = 0usize;
        'dois_saltos: for oi in 0..self.center_orbits.len() {
            if self.base3[oi].is_none() {
                continue;
            }
            // mesma regra da busca direta: a cor certa vem do centro do meio
            let cor = |s: &SN, i: usize| s.cent[oi * 24 + i];
            let face_da_cor = |c: u8| (0..6).find(|&f| cs.mid[f] == c).unwrap_or(0);
            let erradas: Vec<usize> =
                (0..24).filter(|&i| cor(cs, i) != cs.mid[i / 4]).collect();
            // Ordem importa: as casas ERRADAS primeiro. Percorrer as 24 casas
            // em ordem numerica esgotava o teto antes de chegar nas uteis.
            let ordem: Vec<usize> = erradas
                .iter()
                .copied()
                .chain((0..24).filter(|k| !erradas.contains(k)))
                .collect();
            for &p in &erradas {
                let destino = face_da_cor(cor(cs, p));
                for &q in erradas.iter().filter(|&&q| q != p && q / 4 == destino) {
                    for &m in &ordem {
                        if m == p || m == q {
                            continue;
                        }
                        for &r1 in &ordem {
                            if r1 == p || r1 == m {
                                continue;
                            }
                            let Some(s1) = self.cycle_triple(oi, [p as u8, m as u8, r1 as u8])
                            else {
                                continue;
                            };
                            let mut e1 = *cs;
                            for &mv in &s1 {
                                e1 = self.capply(&e1, mv);
                            }
                            for &r2 in &ordem {
                                if r2 == m || r2 == q {
                                    continue;
                                }
                                avaliadas += 1;
                                if avaliadas > teto {
                                    break 'dois_saltos;
                                }
                                let Some(s2) =
                                    self.cycle_triple(oi, [m as u8, q as u8, r2 as u8])
                                else {
                                    continue;
                                };
                                let mut e2 = e1;
                                for &mv in &s2 {
                                    e2 = self.capply(&e2, mv);
                                }
                                if self.center_total(&e2) > total {
                                    let mut junta = s1.clone();
                                    junta.extend(s2);
                                    return Some(junta);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Familia "fatia, encaixa, desfaz": `S · A · S'`, com S uma fatia e A uma
    /// palavra de giros de FACE. Serve para centros e para asas, porque um giro
    /// de face externa nao tira nenhum centro da sua face nem separa um par de
    /// asas — so a fatia S mexe nisso, e ela e desfeita no fim.
    fn slice_face_macro<G: Fn(&SN) -> bool + Sync>(
        &self,
        cs: &SN,
        goal: &G,
        max_a: usize,
    ) -> Option<Vec<usize>> {
        self.slice_face_macro_camada(cs, goal, max_a, None)
    }

    /// `camada`: restringe as fatias a uma profundidade. Para agrupar a orbita
    /// `oi`, so a fatia da camada `oi+1` mexe nas asas certas — varrer as
    /// outras e custo puro (MEDIDO: o degrau de lote sem o filtro levou o 7x7
    /// de 3.6s para 17.2s, porque cada tentativa fracassada pagava a varredura
    /// completa antes de cair no 3-ciclo).
    fn slice_face_macro_camada<G: Fn(&SN) -> bool + Sync>(
        &self,
        cs: &SN,
        goal: &G,
        max_a: usize,
        camada: Option<usize>,
    ) -> Option<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let faces: Vec<usize> = (0..6)
            .flat_map(|f| (0..3).map(move |pw| (f * self.depths) * 3 + pw))
            .collect();
        let slices: Vec<usize> = (0..6)
            .flat_map(|f| {
                (1..self.depths)
                    .filter(move |&d| camada.is_none_or(|c| c == d))
                    .flat_map(move |d| (0..3).map(move |pw| ((f * self.depths) + d) * 3 + pw))
            })
            .collect();

        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let found: Mutex<Option<Vec<usize>>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let cursor = AtomicUsize::new(0);
        let workers = n_workers();
        std::thread::scope(|sc| {
            for _ in 0..workers {
                let (found, stop, cursor) = (&found, &stop, &cursor);
                let (faces, slices) = (&faces, &slices);
                sc.spawn(move || {
                    // dfs sobre A, com o desfazer da fatia embutido no teste
                    fn dfs(
                        cn: &CubeN,
                        s: &SN,
                        depth: usize,
                        path: &mut Vec<usize>,
                        faces: &[usize],
                        undo: usize,
                        goal: &(dyn Fn(&SN) -> bool + Sync),
                        stop: &AtomicBool,
                    ) -> bool {
                        if goal(&cn.capply(s, undo)) {
                            return true;
                        }
                        if depth == 0 || stop.load(Ordering::Relaxed) {
                            return false;
                        }
                        for &m in faces {
                            if let Some(&prev) = path.last() {
                                if prev / 3 == m / 3 {
                                    continue;
                                }
                            }
                            let s2 = cn.capply(s, m);
                            path.push(m);
                            if dfs(cn, &s2, depth - 1, path, faces, undo, goal, stop) {
                                return true;
                            }
                            path.pop();
                        }
                        false
                    }
                    loop {
                        let i = cursor.fetch_add(1, Ordering::Relaxed);
                        if i >= slices.len() || stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let sl = slices[i];
                        let s0 = self.capply(cs, sl);
                        let undo = inv(sl);
                        let mut path = Vec::new();
                        if dfs(self, &s0, max_a, &mut path, faces, undo, goal, stop) {
                            let mut seq = vec![sl];
                            seq.extend(path.iter().copied());
                            seq.push(undo);
                            let mut g = found.lock().unwrap();
                            if g.is_none() {
                                *g = Some(seq);
                            }
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                });
            }
        });
        found.into_inner().unwrap()
    }

    /// Enumeracao de macros para um objetivo generico.
    fn macro_search<G: Fn(&SN) -> bool + Sync>(
        &self,
        cs: &SN,
        goal: &G,
        bs: &[Vec<usize>],
        max_tier: usize,
        post: Option<usize>,
    ) -> Option<Vec<usize>> {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        let applyw = |s: &SN, w: &[usize]| {
            let mut o = *s;
            for &m in w {
                o = self.capply(&o, m);
            }
            o
        };
        let invw = |w: &[usize]| -> Vec<usize> { w.iter().rev().map(|&m| inv(m)).collect() };
        // objetivo avaliado depois do pos-movimento (para "t macro t'")
        let goal = &move |s: &SN| match post {
            Some(pm) => goal(&self.capply(s, pm)),
            None => goal(s),
        };
        // Para a palavra W (estado sw = cs depois de W), testa:
        //   A: W B W'      B: W B W' B'      C: V (a B a' B') V'  com W = V a
        // onde B tambem pode ser uma palavra (fatia + giro, p.ex.).
        let bs_inv: Vec<Vec<usize>> = bs.iter().map(|b| invw(b)).collect();
        let try_word = |word: &[usize], sw: &SN| -> Option<Vec<usize>> {
            let wi = invw(word); // fora do laco: seria uma alocacao por candidato
            let wi_head = invw(&word[..word.len().saturating_sub(1)]);
            for (bi_idx, b) in bs.iter().enumerate() {
                if let (Some(&last), Some(&b0)) = (word.last(), b.first()) {
                    if last / 3 == b0 / 3 {
                        continue;
                    }
                }
                let bi = &bs_inv[bi_idx];
                let sb = applyw(sw, b);
                let s = applyw(&sb, &wi);
                if goal(&s) {
                    let mut seq = word.to_vec();
                    seq.extend_from_slice(b);
                    seq.extend(wi.iter().copied());
                    return Some(seq);
                }
                if goal(&applyw(&s, bi)) {
                    let mut seq = word.to_vec();
                    seq.extend_from_slice(b);
                    seq.extend(wi.iter().copied());
                    seq.extend(bi.iter().copied());
                    return Some(seq);
                }
                if let Some(&a) = word.last() {
                    let s = applyw(&applyw(&sb, &[inv(a)]), bi);
                    let s = applyw(&s, &wi_head);
                    if goal(&s) {
                        let mut seq = word.to_vec();
                        seq.extend_from_slice(b);
                        seq.push(inv(a));
                        seq.extend(bi.iter().copied());
                        seq.extend(wi_head.iter().copied());
                        return Some(seq);
                    }
                }
            }
            None
        };
        if let Some(r) = try_word(&[], cs) {
            return Some(r);
        }
        for m1 in 0..self.n_moves {
            let s1 = self.capply(cs, m1);
            if let Some(r) = try_word(&[m1], &s1) {
                return Some(r);
            }
        }
        // niveis 2 e 3: paralelo por m1, o primeiro achado vence
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let workers = n_workers();
        for tier3 in [false, true] {
            if (tier3 && max_tier < 3) || (!tier3 && max_tier < 2) {
                continue;
            }
            let found: Mutex<Option<Vec<usize>>> = Mutex::new(None);
            let stop = AtomicBool::new(false);
            let cursor = AtomicUsize::new(0);
            std::thread::scope(|sc| {
                for _ in 0..workers {
                    let found = &found;
                    let stop = &stop;
                    let cursor = &cursor;
                    let try_word = &try_word;
                    sc.spawn(move || loop {
                        let m1 = cursor.fetch_add(1, Ordering::Relaxed);
                        if m1 >= self.n_moves || stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let s1 = self.capply(cs, m1);
                        for m2 in 0..self.n_moves {
                            if m2 / 3 == m1 / 3 || stop.load(Ordering::Relaxed) {
                                continue;
                            }
                            let s2 = self.capply(&s1, m2);
                            let hit = if tier3 {
                                let mut r = None;
                                for m3 in 0..self.n_moves {
                                    if m3 / 3 == m2 / 3 {
                                        continue;
                                    }
                                    let s3 = self.capply(&s2, m3);
                                    r = try_word(&[m1, m2, m3], &s3);
                                    if r.is_some() {
                                        break;
                                    }
                                }
                                r
                            } else {
                                try_word(&[m1, m2], &s2)
                            };
                            if let Some(r) = hit {
                                let mut g = found.lock().unwrap();
                                if g.is_none() {
                                    *g = Some(r);
                                }
                                stop.store(true, Ordering::Relaxed);
                                return;
                            }
                        }
                    });
                }
            });
            if let Some(r) = found.into_inner().unwrap() {
                return Some(r);
            }
        }
        None
    }

    fn wing_positions(&self, s: &SN, oi: usize, t: usize) -> (usize, usize) {
        let mut found = [255usize; 2];
        let mut k = 0;
        for q in 0..24 {
            if s.wt[oi * 24 + q] as usize == t {
                if k < 2 {
                    found[k] = q;
                }
                k += 1;
            }
        }
        (found[0], found[1])
    }

    fn pair_h(&self, s: &SN, oi: usize, t: usize) -> u8 {
        let (a, bb) = self.wing_positions(s, oi, t);
        let rel = (((s.wo[oi] >> a) ^ (s.wo[oi] >> bb)) & 1) as usize;
        let d = &self.pair_dist[oi];
        d[(a * 24 + bb) * 2 + rel].min(d[(bb * 24 + a) * 2 + rel])
    }

    fn pair2_h(&self, s: &SN, oi: usize, t1: usize, t2: usize) -> u8 {
        let idx = |a1: usize, b1: usize, r1: usize, a2: usize, b2: usize, r2: usize| {
            (((a1 * 24 + b1) * 2 + r1) * 576 + (a2 * 24 + b2)) * 2 + r2
        };
        let (p1, q1) = self.wing_positions(s, oi, t1);
        let (p2, q2) = self.wing_positions(s, oi, t2);
        let rel = |a: usize, bb: usize| (((s.wo[oi] >> a) ^ (s.wo[oi] >> bb)) & 1) as usize;
        let (r1, r2) = (rel(p1, q1), rel(p2, q2));
        let d = &self.pair2_dist[oi];
        let mut best = 255u8;
        for (a1, b1) in [(p1, q1), (q1, p1)] {
            for (a2, b2) in [(p2, q2), (q2, p2)] {
                best = best.min(d[idx(a1, b1, r1, a2, b2, r2)]);
            }
        }
        best
    }

    /// Verificacao do estado compacto contra a planificacao real.
    fn verify_compact(&self) {
        let mut f = self.solved();
        let mut cs = self.cstate_of(&f);
        let mut seed = 0x0dd0_ba11_cafe_f00du64;
        for _ in 0..200 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let m = ((seed >> 33) % self.n_moves as u64) as usize;
            self.apply(&mut f, m);
            cs = self.capply(&cs, m);
            assert!(cs == self.cstate_of(&f), "estado compacto divergiu (N={})", self.n);
        }
        // predicado de agrupamento compacto == verdade da planificacao
        for oi in 0..self.wing_orbits.len() {
            for t in 0..12 {
                let compact = self.grouped(&cs, oi, t);
                let real = self.grouped_facelets(&f, oi, t);
                assert_eq!(
                    compact, real,
                    "predicado de agrupamento divergiu (N={}, orbita {oi}, tipo {t})",
                    self.n
                );
            }
        }
    }

    /// Verdade absoluta do agrupamento, direto nos adesivos.
    fn grouped_facelets(&self, state: &[u8], oi: usize, t: usize) -> bool {
        let orbit = &self.wing_orbits[oi];
        let solved = self.solved();
        let tmap = self.type_map();
        for j in 0..12 {
            let s0 = (state[orbit[2 * j][0]], state[orbit[2 * j][1]]);
            let s1 = (state[orbit[2 * j + 1][0]], state[orbit[2 * j + 1][1]]);
            if s0 != s1 {
                continue;
            }
            if tmap[s0.0 as usize][s0.1 as usize] as usize != t {
                continue;
            }
            // referencia na mesma casa mostrando as mesmas cores
            if let Some(mf) = &self.midge_facelets {
                let mm = (state[mf[j][0]], state[mf[j][1]]);
                if mm == s0 {
                    return true;
                }
            } else if oi == 0 {
                return true;
            } else {
                let o0 = &self.wing_orbits[0];
                let r0 = (state[o0[2 * j][0]], state[o0[2 * j][1]]);
                let r1 = (state[o0[2 * j + 1][0]], state[o0[2 * j + 1][1]]);
                if r0 == r1 && r0 == s0 {
                    return true;
                }
            }
        }
        let _ = solved;
        false
    }
}

// ---------------------------------------------------------------------------
// Busca IDA* com raiz paralela (generica no objetivo/heuristica)
// ---------------------------------------------------------------------------

struct NSearch<'a, G, H>
where
    G: Fn(&SN) -> bool + Sync,
    H: Fn(&SN) -> u8 + Sync,
{
    cn: &'a CubeN,
    goal: &'a G,
    h: &'a H,
    path: Vec<usize>,
    nodes: usize,
    stop: &'a std::sync::atomic::AtomicBool,
}

impl<'a, G, H> NSearch<'a, G, H>
where
    G: Fn(&SN) -> bool + Sync,
    H: Fn(&SN) -> u8 + Sync,
{
    fn dfs(&mut self, s: &SN, depth: usize) -> bool {
        self.nodes += 1;
        if self.nodes & 0xFFF == 0 && self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        if self.nodes > 20_000_000 {
            return false;
        }
        let h = (self.h)(s) as usize;
        if h > depth {
            return false;
        }
        if h == 0 && (self.goal)(s) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        for m in 0..self.cn.n_moves {
            if let Some(&prev) = self.path.last() {
                let (pg, g) = (prev / 3, m / 3);
                if pg == g {
                    continue; // mesma camada em sequencia
                }
                // camadas do mesmo eixo comutam: so em ordem crescente
                let (pf, f) = (pg / self.cn.depths, g / self.cn.depths);
                if pf % 3 == f % 3 && pg > g {
                    continue;
                }
            }
            let s2 = self.cn.capply(s, m);
            self.path.push(m);
            if self.dfs(&s2, depth - 1) {
                return true;
            }
            self.path.pop();
        }
        false
    }

    fn run_at(cn: &CubeN, start: &SN, goal: &G, h: &H, d: usize) -> Option<Vec<usize>> {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        if h(start) as usize > d {
            return None;
        }
        if d == 0 {
            return if goal(start) { Some(Vec::new()) } else { None };
        }
        let workers = n_workers();
        let found: Mutex<Option<Vec<usize>>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let cursor = AtomicUsize::new(0);
        std::thread::scope(|sc| {
            for _ in 0..workers {
                let found = &found;
                let stop = &stop;
                let cursor = &cursor;
                sc.spawn(move || loop {
                    let m = cursor.fetch_add(1, Ordering::Relaxed);
                    if m >= cn.n_moves || stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let s2 = cn.capply(start, m);
                    let mut se = NSearch { cn, goal, h, path: vec![m], nodes: 0, stop };
                    if se.dfs(&s2, d - 1) {
                        let mut g = found.lock().unwrap();
                        if g.is_none() {
                            *g = Some(se.path.clone());
                        }
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                });
            }
        });
        found.into_inner().unwrap()
    }

    fn run(cn: &CubeN, start: &SN, goal: &G, h: &H, cap: usize) -> Option<Vec<usize>> {
        let h0 = h(start) as usize;
        if h0 == 0 && goal(start) {
            return Some(Vec::new());
        }
        for d in h0.max(1)..=cap {
            if let Some(seq) = Self::run_at(cn, start, goal, h, d) {
                return Some(seq);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg_scramble(cn: &CubeN, seed: u64, len: usize) -> Vec<u8> {
        let mut st = cn.solved();
        let mut s = seed;
        let mut last = usize::MAX;
        let mut k = 0;
        while k < len {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let m = ((s >> 33) % cn.n_moves as u64) as usize;
            if m / 3 == last {
                continue;
            }
            last = m / 3;
            cn.apply(&mut st, m);
            k += 1;
        }
        st
    }

    /// O 3x3 reduzido so aceita um numero PAR de pares invertidos (com impar ele
    /// acusa "uma aresta esta invertida"), e a orientacao NAO pode ser escolhida
    /// ao fechar a ultima aresta — cada asa tem quiralidade fixa. Logo a correcao
    /// precisa ser uma sequencia que vire uma aresta no lugar.
    ///
    /// Vira a aresta INTEIRA, e nao so uma orbita: virar metade poe as orbitas em
    /// desacordo, a aresta desagrupa, o agrupamento chama a mesma sequencia para
    /// reorientar e a paridade volta ao ponto de partida. Medido no 6x6, esse
    /// ciclo de periodo 2 so terminava esgotando as rodadas e refazendo o cubo
    /// do zero (7 aplicacoes, ~5s jogados fora por tentativa).
    /// O que a correcao certificada faz, aresta por aresta: quais casas ela
    /// inverte em cada orbita. Se as orbitas nao inverterem A MESMA aresta, a
    /// aresta nao esta virando inteira — esta virando metade aqui e metade ali.
    #[test]
    #[ignore = "diagnóstico: retrato da correção de paridade"]
    fn retrato_da_correcao_de_paridade() {
        for n in [4usize, 6] {
            let cn = cuben(n);
            let base = cn.cstate_of(&cn.solved());
            let Some(seq) = cn.flip_alg.clone() else {
                println!("N={n}: sem correcao");
                continue;
            };
            let mut s = base;
            for &m in &seq {
                s = cn.capply(&s, m);
            }
            println!("\nN={n}: {} movimentos", seq.len());
            for oi in 0..cn.wing_orbits.len() {
                let virou: Vec<usize> = (0..12)
                    .filter(|&j| (s.wo[oi] >> (2 * j)) & 1 != (base.wo[oi] >> (2 * j)) & 1)
                    .collect();
                let desagrupou: Vec<usize> =
                    (0..12).filter(|&t| !cn.grouped(&s, oi, t)).collect();
                let mudou_casa: Vec<usize> = (0..24)
                    .filter(|&q| s.wt[oi * 24 + q] != base.wt[oi * 24 + q])
                    .collect();
                println!(
                    "  orbita {oi}: inverteu as arestas {virou:?}, desagrupou {desagrupou:?}, casas mexidas {}",
                    mudou_casa.len()
                );
            }
        }
    }

    /// Guarda o que o build tem de entregar nos cubos pares: existe correcao, ela
    /// nao toca nos centros e troca a paridade dos pares invertidos.
    ///
    /// NAO exige que a aresta vire inteira. E tentador — a sequencia coerente
    /// existe (largura 3 no 6x6) e parece mais limpa —, mas foi medido: 6 casos
    /// passaram de 37.9s para 96.0s. O comentario no build explica por que.
    #[test]
    fn correcao_de_paridade_certificada() {
        for n in [4usize, 6] {
            let cn = cuben(n);
            let seq = cn
                .flip_alg
                .clone()
                .unwrap_or_else(|| panic!("N={n}: build nao certificou nenhuma correcao"));
            let base = cn.cstate_of(&cn.solved());
            let mut s = base;
            for &m in &seq {
                s = cn.capply(&s, m);
            }
            assert_eq!(s.cent, base.cent, "N={n}: a correcao mexeu nos centros");
            assert_eq!(
                cn.invertidos(&s, 0) % 2,
                1,
                "N={n}: a correcao nao trocou a paridade"
            );
            println!("N={n}: correcao com {} movimentos", seq.len());
        }
    }

    /// Nos cubos ímpares, os centros do meio só se movem com fatias profundas,
    /// que desarrumam as órbitas — então, quando só eles ficam fora do lugar, o
    /// solver empaca (medido: 146/150 no 7x7, com as 144 peças de órbita já
    /// certas). Procura uma sequência que 3-cicle os centros do meio SEM mexer
    /// nas órbitas; achando, o fim de jogo deixa de ser busca.
    #[test]
    fn acha_3ciclo_dos_centros_do_meio() {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        for n in [5usize, 7] {
            let cn = cuben(n);
            let base = cn.cstate_of(&cn.solved());
            let atomos = cn.atoms();
            let mut achou: Option<(String, Vec<u8>)> = None;
            'busca: for a1 in &atomos {
                for a2 in &atomos {
                    // W = a1 a2, b = a2' — comutadores curtos sobre os atomos
                    for b in &atomos {
                        let mut seq: Vec<usize> = a1.clone();
                        seq.extend(a2.iter().copied());
                        seq.extend(b.iter().copied());
                        seq.extend(a2.iter().rev().map(|&m| inv(m)));
                        seq.extend(a1.iter().rev().map(|&m| inv(m)));
                        seq.extend(b.iter().rev().map(|&m| inv(m)));
                        let mut s = base;
                        for &m in &seq {
                            s = cn.capply(&s, m);
                        }
                        if s.cent != base.cent || s.mid == base.mid {
                            continue; // mexeu nas orbitas, ou nao mexeu no meio
                        }
                        let mexidos = (0..6).filter(|&f| s.mid[f] != base.mid[f]).count();
                        if mexidos == 3 {
                            achou = Some((
                                seq.iter()
                                    .map(|&m| cn.move_name(m))
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                s.mid.to_vec(),
                            ));
                            break 'busca;
                        }
                    }
                }
            }
            match achou {
                Some((seq, mid)) => println!("N={n}: 3-ciclo dos meios: {seq}  -> mid {mid:?}"),
                None => println!("N={n}: NAO achei 3-ciclo puro dos centros do meio"),
            }
        }
    }

    /// O 3-ciclo construído só alcança os trios que a árvore de conjugação
    /// cobre. Se a cobertura for parcial, há peças que ele NUNCA arruma — e
    /// nenhuma reordenação de busca resolve isso. Mede a cobertura por órbita.
    #[test]
    fn cobertura_das_arvores_de_trio() {
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            let total_trios = 24 * 23 * 22; // trios de casas distintas
            println!("\nN={n} (trios distintos: {total_trios}):");
            for (oi, arvore) in cn.triple_trees.iter().enumerate() {
                if arvore.is_empty() {
                    println!("  centros órbita {oi}: sem árvore");
                    continue;
                }
                let alcancados = arvore.iter().filter(|(_, m, _)| *m != u8::MAX).count();
                println!(
                    "  centros órbita {oi}: {alcancados} de {total_trios} ({:.0}%)",
                    alcancados as f64 * 100.0 / total_trios as f64
                );
            }
            for (oi, arvore) in cn.wing_trees.iter().enumerate() {
                if arvore.is_empty() {
                    println!("  asas órbita {oi}: sem árvore");
                    continue;
                }
                let alcancados = arvore.iter().filter(|(_, m, _)| *m != u8::MAX).count();
                println!(
                    "  asas órbita {oi}: {alcancados} de {total_trios} ({:.0}%)",
                    alcancados as f64 * 100.0 / total_trios as f64
                );
            }
        }
    }

    /// O caso lento do 7x7 (semente 1072 da régua) leva ~9 minutos. Este teste
    /// mostra ONDE ele gasta: em que contagem de centros/asas empaca, quantos
    /// reinícios faz e quanto custa cada degrau.
    /// Onde o 6x6 gasta o tempo (hoje o mais lento dos tres). Sendo par, ele
    /// nao tem centro fixo de referencia, entao depende das correcoes de
    /// paridade — a suspeita e que o custo esteja ai. Rodar com CUBEN_DEBUG=1.
    #[test]
    #[ignore = "diagnóstico do 6x6"]
    fn diagnostico_6x6_pipeline() {
        let tables = Tables::build();
        let cn = cuben(6);
        let mut total = 0.0;
        let mut movs = 0usize;
        for caso in 0..6 {
            let st = lcg_scramble(&cn, 1000 + (60 + caso) as u64, 60);
            let entrada = cn.render(&st, &['U', 'R', 'F', 'D', 'L', 'B']);
            let t0 = std::time::Instant::now();
            let sol = solve_n(6, &entrada, &tables).expect("resolver");
            total += t0.elapsed().as_secs_f64();
            movs += sol.length;
            println!(
                "\n6x6 caso {caso}: {} movimentos em {:.1}s, {} etapas",
                sol.length,
                t0.elapsed().as_secs_f64(),
                sol.stages.len()
            );
            let mut por_nome: Vec<(String, usize)> = Vec::new();
            for s in &sol.stages {
                let chave = s.name.split(" (").next().unwrap_or(&s.name).to_string();
                match por_nome.iter_mut().find(|(n, _)| *n == chave) {
                    Some((_, q)) => *q += s.tokens.len(),
                    None => por_nome.push((chave, s.tokens.len())),
                }
            }
            por_nome.sort_by_key(|(_, q)| std::cmp::Reverse(*q));
            for (nome, q) in por_nome.iter().take(3) {
                println!("  {q:5} movimentos — {nome}");
            }
        }
        println!("\nTOTAL 6x6: {total:.1}s, {movs} movimentos");
    }

    /// Anatomia do 3-ciclo: cada ciclo custa |B| + 2|V| (base pura + conjugacao
    /// de ida e volta). Mede |B| por orbita e a distribuicao de |V| sobre todos
    /// os trios alcancaveis — e o mapa para encurtar o ciclo em si.
    #[test]
    #[ignore = "diagnóstico: anatomia do 3-ciclo"]
    fn anatomia_do_3ciclo() {
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            println!("\nN={n}:");
            for (nome, bases, arvores) in [
                ("centros", &cn.base3, &cn.triple_trees),
                ("asas", &cn.wbase3, &cn.wing_trees),
            ] {
                for (oi, b) in bases.iter().enumerate() {
                    let Some(b) = b else { continue };
                    let arv = &arvores[oi];
                    if arv.is_empty() {
                        continue;
                    }
                    // profundidade de cada trio alcancavel
                    let mut profs: Vec<usize> = Vec::new();
                    for i in 0..arv.len() {
                        if arv[i].1 >= u8::MAX - 1 {
                            continue; // raiz ou inalcancavel
                        }
                        let mut d = 0;
                        let mut a = i;
                        while arv[a].1 != u8::MAX - 1 {
                            d += 1;
                            a = arv[a].0 as usize;
                        }
                        profs.push(d);
                    }
                    profs.sort_unstable();
                    let media = profs.iter().sum::<usize>() as f64 / profs.len().max(1) as f64;
                    let mediana = profs.get(profs.len() / 2).copied().unwrap_or(0);
                    let max = profs.last().copied().unwrap_or(0);
                    println!(
                        "  {nome} orbita {oi}: |B|={} |V| media={media:.1} mediana={mediana} \
                         max={max} => ciclo tipico {:.0} movimentos",
                        b.len(),
                        b.len() as f64 + 2.0 * media
                    );
                }
            }
        }
    }

    /// Raio-X do COMPRIMENTO: para onde vao os movimentos, por fase, nos tres
    /// tamanhos. E o mapa para encurtar a solucao (hoje ~900 no 6x6 e ~1250 no
    /// 7x7 contra ~200 de um humano): so vale atacar a fase que domina.
    #[test]
    #[ignore = "diagnóstico: movimentos por fase"]
    fn raio_x_do_comprimento() {
        let tables = Tables::build();
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            // soma por categoria em 3 casos fixos
            let mut cat: Vec<(&str, usize)> =
                vec![("centros", 0), ("arestas", 0), ("paridade", 0), ("3x3", 0)];
            let mut total = 0usize;
            let mut arestas_feitas = 0usize;
            for caso in 0..3 {
                let st = lcg_scramble(&cn, 1000 + (n * 10 + caso) as u64, n * 10);
                let entrada = cn.render(&st, &['U', 'R', 'F', 'D', 'L', 'B']);
                let sol = solve_n(n, &entrada, &tables).expect("resolver");
                total += sol.length;
                for s in &sol.stages {
                    let idx = if s.name.starts_with("Montar") {
                        0
                    } else if s.name.starts_with("Agrupar") {
                        arestas_feitas += 1;
                        1
                    } else if s.name.contains("aridade") {
                        2
                    } else {
                        3
                    };
                    cat[idx].1 += s.tokens.len();
                }
            }
            println!("\n{n}x{n} — {total} movimentos em 3 casos ({} por cubo):", total / 3);
            for (nome, q) in &cat {
                println!(
                    "  {q:5} ({:4.1}%) — {nome}",
                    100.0 * *q as f64 / total as f64
                );
            }
            if arestas_feitas > 0 {
                println!(
                    "  custo por aresta agrupada: {:.1} movimentos",
                    cat[1].1 as f64 / arestas_feitas as f64
                );
            }
        }
    }

    /// O mesmo caso, mas pelo pipeline inteiro: mede quanto vai para centros,
    /// asas e paridade. Rodar com CUBEN_DEBUG=1 para ver os degraus.
    #[test]
    #[ignore = "diagnóstico do caso lento do 7x7 (pipeline completo)"]
    fn diagnostico_7x7_pipeline() {
        let tables = Tables::build();
        let cn = cuben(7);
        let st = lcg_scramble(&cn, 1072, 70);
        let entrada = cn.render(&st, &['U', 'R', 'F', 'D', 'L', 'B']);
        let t0 = std::time::Instant::now();
        let sol = solve_n(7, &entrada, &tables).expect("resolver");
        println!(
            "7x7 semente 1072: {} movimentos em {:.1}s, {} etapas",
            sol.length,
            t0.elapsed().as_secs_f64(),
            sol.stages.len()
        );
        // quanto foi para cada tipo de etapa
        let mut por_nome: Vec<(String, usize)> = Vec::new();
        for s in &sol.stages {
            let chave = s.name.split(" (").next().unwrap_or(&s.name).to_string();
            match por_nome.iter_mut().find(|(n, _)| *n == chave) {
                Some((_, q)) => *q += s.tokens.len(),
                None => por_nome.push((chave, s.tokens.len())),
            }
        }
        por_nome.sort_by_key(|(_, q)| std::cmp::Reverse(*q));
        for (nome, q) in por_nome {
            println!("  {q:5} movimentos — {nome}");
        }
    }

    /// Reproduz a fase de centros COMO O PIPELINE faz (a partir do estado
    /// interpretado, não do embaralhado cru) e, na primeira vez que o passo
    /// construtivo falha, mostra exatamente quais peças estão fora e onde.
    #[test]
    #[ignore = "diagnóstico: por que os centros empacam em 146/150"]
    fn diagnostico_centros_travados() {
        let cn = cuben(7);
        let bruto = lcg_scramble(&cn, 1072, 70);
        let entrada = cn.render(&bruto, &['U', 'R', 'F', 'D', 'L', 'B']);
        let (mut state, _letras) = cn.parse(&entrada).expect("interpretar");
        let alvo = cn.center_total_max();
        for passo in 0..400 {
            let cs = cn.cstate_of(&state);
            let total = cn.center_total(&cs);
            if total == alvo {
                println!("centros fecharam em {passo} passos");
                return;
            }
            match cn.improve_centers(&cs, total) {
                Some(seq) => {
                    for m in seq {
                        cn.apply(&mut state, m);
                    }
                }
                None => {
                    println!("\nEMPACOU em {total}/{alvo} (passo {passo}):");
                    for oi in 0..cn.center_orbits.len() {
                        let erradas: Vec<String> = (0..24)
                            .filter(|&i| cs.cent[oi * 24 + i] as usize != i / 4)
                            .map(|i| {
                                format!(
                                    "casa {i} (face {}) tem cor {}",
                                    i / 4,
                                    cs.cent[oi * 24 + i]
                                )
                            })
                            .collect();
                        if !erradas.is_empty() {
                            println!("  órbita {oi}: {}", erradas.join("; "));
                        }
                    }
                    let cons = cn.constructive_center_step_teto(&cs, 500_000).is_some();
                    println!("  construtivo com teto alto acha algo? {cons}");
                    return;
                }
            }
        }
        println!("nao empacou em 400 passos");
    }

    #[test]
    #[ignore = "diagnóstico do caso lento do 7x7"]
    fn diagnostico_7x7_lento() {
        let cn = cuben(7);
        let mut state = lcg_scramble(&cn, 1072, 70);
        let alvo = cn.center_total_max();
        let mut passos = 0;
        let mut travas = 0;
        let t0 = std::time::Instant::now();
        let mut ultimo_total = 0usize;
        let mut repeticoes = 0;
        loop {
            let cs = cn.cstate_of(&state);
            let total = cn.center_total(&cs);
            if total == alvo {
                println!(
                    "centros fecharam em {passos} passos, {travas} sem avanço, {:.1}s",
                    t0.elapsed().as_secs_f64()
                );
                break;
            }
            if passos > 400 {
                println!(
                    "DESISTIU em {total}/{alvo} apos {passos} passos ({:.1}s)",
                    t0.elapsed().as_secs_f64()
                );
                break;
            }
            passos += 1;
            if total == ultimo_total {
                repeticoes += 1;
                if repeticoes % 20 == 0 {
                    println!("  preso em {total}/{alvo} ha {repeticoes} passos");
                }
            } else {
                ultimo_total = total;
                repeticoes = 0;
            }
            let t = std::time::Instant::now();
            match cn.improve_centers(&cs, total) {
                Some(seq) => {
                    if t.elapsed().as_secs_f64() > 1.0 {
                        println!("  {total}/{alvo}: {} mov em {:.1}s", seq.len(), t.elapsed().as_secs_f64());
                    }
                    for m in seq {
                        cn.apply(&mut state, m);
                    }
                }
                None => {
                    travas += 1;
                    println!(
                        "  SEM SAIDA em {total}/{alvo} ({:.1}s nesta busca)",
                        t.elapsed().as_secs_f64()
                    );
                    let ch = cn.plateau_shuffle(&cs, total, travas, &[]);
                    for m in ch {
                        cn.apply(&mut state, m);
                    }
                }
            }
        }
    }

    /// Régua fixa para comparar mudanças: embaralhamentos com semente
    /// constante, então antes/depois medem O MESMO cubo. Sem isso a variação
    /// entre embaralhamentos esconde (ou inventa) ganhos — foi assim que
    /// aprovei uma "melhoria" que na verdade piorava.
    /// Rodar: cargo test --release regua_cubos_grandes -- --ignored --nocapture
    #[test]
    #[ignore = "régua de comparação; roda sob demanda"]
    fn regua_cubos_grandes() {
        let tables = Tables::build();
        let letras = ['U', 'R', 'F', 'D', 'L', 'B'];
        println!("\n tamanho | caso | movimentos | tempo");
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            let mut soma_mov = 0usize;
            let mut soma_s = 0.0;
            let casos = 3;
            for caso in 0..casos {
                let st = lcg_scramble(&cn, 1000 + (n * 10 + caso) as u64, 10 * n);
                let entrada = cn.render(&st, &letras);
                let t0 = std::time::Instant::now();
                let sol = solve_n(n, &entrada, &tables)
                    .unwrap_or_else(|e| panic!("N={n} caso {caso}: {e}"));
                let s = t0.elapsed().as_secs_f64();
                // confere de verdade: as seis faces uniformes
                let ult: Vec<char> = sol.states.last().unwrap().chars().collect();
                let por_face = n * n;
                for f in 0..6 {
                    let c0 = ult[f * por_face];
                    assert!(
                        (0..por_face).all(|k| ult[f * por_face + k] == c0),
                        "N={n} caso {caso}: face {f} nao uniforme"
                    );
                }
                println!("   {n}x{n}  |  {caso}   |    {:5}   | {s:6.1}s", sol.length);
                soma_mov += sol.length;
                soma_s += s;
            }
            println!(
                "   {n}x{n}  | MÉDIA|    {:5}   | {:6.1}s",
                soma_mov / casos,
                soma_s / casos as f64
            );
        }
    }

    /// O 4x4 pela mesma construcao generica: e o solver mais antigo do projeto
    /// (busca IDA* ate profundidade 13 nos centros) e custa ~65s por cubo.
    /// Compara corretude e tempo com o caminho novo.
    #[test]
    fn cubo4_pela_construcao_generica() {
        let tables = Tables::build();
        let cn = cuben(4);
        assert_eq!(cn.n_moves, 36, "4x4 deveria ter 36 movimentos");
        assert_eq!(cn.center_orbits.len(), 1);
        assert_eq!(cn.wing_orbits.len(), 1);
        assert!(cn.midge_facelets.is_none(), "4x4 nao tem aresta do meio");
        assert!(cn.base3[0].is_some(), "sem 3-ciclo puro de centro no 4x4");
        assert!(cn.wbase3[0].is_some(), "sem 3-ciclo puro de asa no 4x4");

        let mut seed = 0xfeed_beef_1234_5678u64;
        for caso in 0..2 {
            let mut st = cn.solved();
            let mut last = usize::MAX;
            let mut k = 0;
            while k < 40 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let m = ((seed >> 33) % cn.n_moves as u64) as usize;
                if m / 3 == last {
                    continue;
                }
                last = m / 3;
                cn.apply(&mut st, m);
                k += 1;
            }
            let entrada = cn.render(&st, &['U', 'R', 'F', 'D', 'L', 'B']);
            let t0 = std::time::Instant::now();
            let sol = solve_n(4, &entrada, &tables).expect("resolver 4x4");
            let gasto = t0.elapsed().as_secs_f64();
            let ultimo = sol.states.last().unwrap();
            let ch: Vec<char> = ultimo.chars().collect();
            for f in 0..6 {
                let c0 = ch[f * 16];
                assert!(
                    (0..16).all(|q| ch[f * 16 + q] == c0),
                    "caso {caso}: face {f} nao uniforme"
                );
            }
            println!("4x4 generico caso {caso}: {} movimentos em {gasto:.1}s", sol.length);
        }
    }

    /// Replica um caso exato: `CUBEN_STATE=<planificacao>` (o tamanho sai do
    /// tamanho da string). Serve para reexecutar qualquer estado que apareceu
    /// num log e depurar so aquele caso.
    #[test]
    fn replicar_estado() {
        let Ok(s) = std::env::var("CUBEN_STATE") else {
            println!("defina CUBEN_STATE=<planificacao> para replicar um caso");
            return;
        };
        let limpo: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        let n = match limpo.len() {
            150 => 5,
            216 => 6,
            294 => 7,
            outro => panic!("{outro} adesivos nao correspondem a 5x5, 6x6 nem 7x7"),
        };
        println!("replicando N={n} com {} adesivos", limpo.len());
        let tables = Tables::build();
        let sol = solve_n(n, &limpo, &tables).expect("resolver");
        let ultimo = sol.states.last().unwrap();
        let por_face = n * n;
        let chars: Vec<char> = ultimo.chars().collect();
        for f in 0..6 {
            let c0 = chars[f * por_face];
            assert!(
                (0..por_face).all(|k| chars[f * por_face + k] == c0),
                "face {f} nao ficou uniforme"
            );
        }
        println!("resolvido em {} movimentos", sol.length);
    }

    /// Existe comutador curto que seja 3-ciclo PURO numa orbita de centros
    /// (identidade nas outras)? Se sim, o fim de jogo dos centros deixa de ser
    /// busca: conjuga-se esse 3-ciclo para as pecas que se quiser.
    /// Com atomos (fatias puras incluidas), toda orbita de centro tem 3-ciclo
    /// puro? E o que decide se o fim de jogo pode ser construido em vez de
    /// buscado.
    #[test]
    fn base_3ciclos_por_atomos() {
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            let t0 = std::time::Instant::now();
            let achado = cn.find_base_3cycles();
            println!("\nN={n} ({:.1}s):", t0.elapsed().as_secs_f64());
            for (oi, a) in achado.iter().enumerate() {
                match a {
                    Some(seq) => println!(
                        "  orbita {oi}: {} mov  {}",
                        seq.len(),
                        seq.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" ")
                    ),
                    None => println!("  orbita {oi}: NAO ACHEI"),
                }
            }
            assert!(
                achado.iter().all(|x| x.is_some()),
                "N={n}: falta 3-ciclo puro em alguma orbita"
            );
        }
    }

    /// Quais movimentos tem sinal IMPAR nas asas? Sao os unicos capazes de
    /// desfazer um par trocado (a "aresta virada" dos cubos grandes).
    #[test]
    fn movimentos_impares_das_asas() {
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            println!("\nN={n}:");
            for oi in 0..cn.wing_orbits.len() {
                let impares = cn.wing_odd_moves(oi);
                let nomes: Vec<String> =
                    impares.iter().take(8).map(|&m| cn.move_name(m)).collect();
                println!(
                    "  orbita {oi}: {} movimentos impares  {}",
                    impares.len(),
                    nomes.join(" ")
                );
            }
        }
    }

    /// Toda orbita de ASAS tem 3-ciclo puro (mexe 3 asas e nada mais)? E o que
    /// permite construir o fim de jogo do agrupamento em vez de procura-lo.
    #[test]
    fn base_3ciclos_de_asas() {
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            println!("\nN={n}: 3-ciclo puro de asas por orbita:");
            for (oi, a) in cn.wbase3.iter().enumerate() {
                match a {
                    Some(seq) => println!(
                        "  orbita {oi}: {} mov  {}   suporte {:?}",
                        seq.len(),
                        seq.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" "),
                        cn.wing_cycle_support(seq, oi)
                    ),
                    None => println!("  orbita {oi}: NAO ACHEI"),
                }
            }
            assert!(
                cn.wbase3.iter().all(|x| x.is_some()),
                "N={n}: falta 3-ciclo puro de asas"
            );
        }
    }

    #[test]
    fn ha_3ciclo_puro_de_centros() {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        for n in [5usize, 6, 7] {
            let cn = cuben(n);
            let base = cn.cstate_of(&cn.solved());
            let n_orb = cn.center_orbits.len();
            // conta pecas de centro mexidas por orbita; None se mexeu no meio
            let efeito = |s: &SN| -> Option<Vec<usize>> {
                if s.mid != base.mid {
                    return None;
                }
                let mut mexidas = vec![0usize; n_orb];
                for oi in 0..n_orb {
                    for i in 0..24 {
                        if s.cent[oi * 24 + i] != base.cent[oi * 24 + i] {
                            mexidas[oi] += 1;
                        }
                    }
                }
                Some(mexidas)
            };
            // [W, b] com |W| = 1..3
            let mut achado: Vec<Option<(usize, String)>> = vec![None; n_orb];
            let mut palavras: Vec<Vec<usize>> = (0..cn.n_moves).map(|m| vec![m]).collect();
            for tamanho in 1..=3usize {
                for w in &palavras {
                    let mut sw = base;
                    for &m in w {
                        sw = cn.capply(&sw, m);
                    }
                    for b in 0..cn.n_moves {
                        if w.last().is_some_and(|&l| l / 3 == b / 3) {
                            continue;
                        }
                        let mut s = cn.capply(&sw, b);
                        for &m in w.iter().rev() {
                            s = cn.capply(&s, inv(m));
                        }
                        s = cn.capply(&s, inv(b));
                        let Some(mexidas) = efeito(&s) else { continue };
                        if mexidas.iter().sum::<usize>() == 3 {
                            let oi = mexidas.iter().position(|&x| x == 3).unwrap();
                            if achado[oi].is_none() {
                                let mut seq: Vec<usize> = w.clone();
                                seq.push(b);
                                seq.extend(w.iter().rev().map(|&m| inv(m)));
                                seq.push(inv(b));
                                achado[oi] = Some((
                                    tamanho,
                                    seq.iter()
                                        .map(|&m| cn.move_name(m))
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                ));
                            }
                        }
                    }
                }
                if achado.iter().all(|x| x.is_some()) {
                    break;
                }
                // proximo tamanho de W
                if tamanho < 3 {
                    let mut nova = Vec::new();
                    for w in &palavras {
                        for m in 0..cn.n_moves {
                            if w.last().is_some_and(|&l| l / 3 == m / 3) {
                                continue;
                            }
                            let mut x = w.clone();
                            x.push(m);
                            nova.push(x);
                        }
                    }
                    palavras = nova;
                }
            }
            println!("\nN={n}: 3-ciclo puro de centro por orbita:");
            for (oi, a) in achado.iter().enumerate() {
                match a {
                    Some((t, seq)) => println!("  orbita {oi}: |W|={t}  {seq}"),
                    None => println!("  orbita {oi}: NAO ACHEI com |W|<=3"),
                }
            }
        }
    }

    /// Mede, no estado onde a subida empaca, QUANTAS familias conseguem
    /// melhorar. Responde se falta cobertura (nenhuma melhora) ou se o
    /// problema e custo/ordem dos degraus (alguma melhora, mas caro).
    /// Ferramenta de investigacao (~26s), fora da suite padrao.
    #[test]
    #[ignore = "diagnostico: rodar quando a fase de centros regredir"]
    fn diagnostico_centros() {
        let inv = |m: usize| (m / 3) * 3 + (2 - m % 3);
        for n in [6usize, 7] {
            let cn = cuben(n);
            let mut state = lcg_scramble(&cn, 12345 + n as u64, 10 * n);
            let alvo = cn.center_total_max();

            // sobe usando SO os degraus baratos, ate empacar
            let baratos = |cs: &SN, total: usize| -> Option<Vec<usize>> {
                let goal = |s: &SN| cn.center_total(s) > total;
                let h = |_: &SN| 0u8;
                for d in 1..=3usize {
                    if let Some(r) = NSearch::run_at(&cn, cs, &goal, &h, d) {
                        return Some(r);
                    }
                }
                for prof in 1..=4usize {
                    if let Some(r) = cn.slice_face_macro(cs, &goal, prof) {
                        return Some(r);
                    }
                }
                None
            };
            let mut passos = 0;
            let travado = loop {
                let cs = cn.cstate_of(&state);
                let total = cn.center_total(&cs);
                if total == alvo {
                    break None;
                }
                match baratos(&cs, total) {
                    Some(seq) => {
                        for m in seq {
                            cn.apply(&mut state, m);
                        }
                        passos += 1;
                    }
                    None => break Some((cs, total)),
                }
            };
            let Some((cs, total)) = travado else {
                println!("N={n}: degraus baratos fecharam tudo em {passos} passos");
                continue;
            };
            println!("\nN={n}: empacou em {total}/{alvo} apos {passos} passos baratos");
            // erradas por orbita
            for oi in 0..cn.center_orbits.len() {
                let erradas =
                    (0..24).filter(|&i| cs.cent[oi * 24 + i] as usize != i / 4).count();
                println!("  orbita {oi}: {erradas} de 24 fora do lugar");
            }

            // quantos comutadores simples [a,b] melhoram?
            let mut simples = 0;
            for a in 0..cn.n_moves {
                for b in 0..cn.n_moves {
                    if a / 3 == b / 3 {
                        continue;
                    }
                    let mut s = cn.capply(&cs, a);
                    s = cn.capply(&s, b);
                    s = cn.capply(&s, inv(a));
                    s = cn.capply(&s, inv(b));
                    if cn.center_total(&s) > total {
                        simples += 1;
                    }
                }
            }
            println!("  comutadores [a,b] que melhoram: {simples}");

            // e conjugados por UM movimento?
            let t0 = std::time::Instant::now();
            let mut conj = 0;
            'busca: for v in 0..cn.n_moves {
                let sv = cn.capply(&cs, v);
                for a in 0..cn.n_moves {
                    for b in 0..cn.n_moves {
                        if a / 3 == b / 3 {
                            continue;
                        }
                        let mut s = cn.capply(&sv, a);
                        s = cn.capply(&s, b);
                        s = cn.capply(&s, inv(a));
                        s = cn.capply(&s, inv(b));
                        s = cn.capply(&s, inv(v));
                        if cn.center_total(&s) > total {
                            conj += 1;
                            if conj >= 5 {
                                break 'busca;
                            }
                        }
                    }
                }
            }
            println!(
                "  conjugados V[a,b]V' que melhoram: {}{} (varredura {:.2}s)",
                conj,
                if conj >= 5 { "+" } else { "" },
                t0.elapsed().as_secs_f64()
            );
        }
    }

    /// Diagnostico da fase de centros: quantos passos e quanto tempo por
    /// tamanho. Fora da suite padrao porque pode oscilar (o 3-ciclo construido
    /// depende de conjugar para um trio, e o grupo nao alcanca todos os trios
    /// em algumas orbitas). No solver isso e tratado reiniciando perturbado; a
    /// garantia de correcao fica com `cubon_reducao_resolve`, que resolve o
    /// cubo inteiro. Rodar com: cargo test centros_sobem -- --ignored
    #[test]
    #[ignore = "diagnostico pesado; pode oscilar por configuracao"]
    fn centros_sobem_ate_o_maximo() {
        // CUBEN_ONLY=6 roda so um tamanho (itera mais rapido no caso difícil)
        let so = std::env::var("CUBEN_ONLY").ok().and_then(|v| v.parse::<usize>().ok());
        for n in [5usize, 6, 7] {
            if so.is_some_and(|x| x != n) {
                continue;
            }
            let cn = cuben(n);
            assert_eq!(
                cn.center_total(&cn.cstate_of(&cn.solved())),
                cn.center_total_max()
            );

            let mut state = lcg_scramble(&cn, 12345 + n as u64, 10 * n);
            let letras = ['U', 'R', 'F', 'D', 'L', 'B'];
            println!("N={n} estado inicial: {}", cn.render(&state, &letras));
            let alvo = cn.center_total_max();
            let mut passos = 0;
            let mut desembaracos = 0;
            let mut vistos: Vec<u64> = Vec::new();
            let t_total = std::time::Instant::now();
            loop {
                let cs = cn.cstate_of(&state);
                let total = cn.center_total(&cs);
                if total == alvo {
                    break;
                }
                vistos.push(cn.center_sig(&cs));
                if vistos.len() > 60 {
                    vistos.remove(0);
                }
                if passos >= 1500 {
                    panic!(
                        "N={n}: nao fechou os centros ({total}/{alvo})\n  estado travado: {}",
                        cn.render(&state, &letras)
                    );
                }
                passos += 1;
                let t0 = std::time::Instant::now();
                match cn.improve_centers(&cs, total) {
                    Some(seq) => {
                        for m in seq {
                            cn.apply(&mut state, m);
                        }
                        let novo = cn.center_total(&cn.cstate_of(&state));
                        assert!(novo > total, "N={n}: a medida nao subiu ({total} -> {novo})");
                        if t0.elapsed().as_secs_f64() > 1.0 {
                            println!(
                                "N={n} {total}->{novo}: {:.2}s",
                                t0.elapsed().as_secs_f64()
                            );
                        }
                    }
                    None => {
                        let k = desembaracos;
                        desembaracos += 1;
                        for m in cn.plateau_shuffle(&cs, total, k, &vistos) {
                            cn.apply(&mut state, m);
                        }
                    }
                }
            }
            println!(
                "N={n}: centros fechados em {passos} passos ({desembaracos} platôs) em {:.1}s",
                t_total.elapsed().as_secs_f64()
            );
        }
    }
}
