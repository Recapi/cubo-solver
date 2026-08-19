//! Estado de um cubo-calendario: o que cada adesivo mostra, e virado para onde.
//!
//! O solver normal trabalha com seis cores e pergunta "esta face esta uniforme?".
//! Aqui cada adesivo traz um desenho proprio — um numero, uma letra, um dia da
//! semana — e a ROTACAO conta: o mesmo 6 de cabeca para baixo vira 9, e e desse
//! truque que o cubo depende para caber sete arranjos de mes em seis centros.
//!
//! Por isso o estado e uma lista de 294 adesivos (7x7 x 6 faces), cada um com
//! simbolo e rotacao. A ordem das casas e a mesma do resto do projeto: face
//! (U R F D L B), linha, coluna.

use std::fmt;

/// O que esta impresso num adesivo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Simbolo {
    /// adesivo em branco
    Vazio,
    /// uma data
    Data(u8),
    /// a casa serve a duas semanas: `24/31`
    Dupla(u8, u8),
    /// pedaco do nome do mes. Cabe mais de uma letra: os CANTOS trazem duas
    /// ("Ma"), enquanto as casas de borda trazem uma. Confirmado com o cubo na
    /// mao — a imagem oficial do site, que mostra uma letra por casa, engana
    /// nesse ponto porque nao marca onde uma peca termina e a outra comeca.
    Letra(Texto),
    /// cabecalho do dia da semana, guardado pelo indice (0 = Sun)
    Dia(u8),
}

/// Um texto curto guardado sem alocar, para o Simbolo continuar Copy.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Texto {
    bytes: [u8; 4],
    tam: u8,
}

impl Texto {
    pub fn novo(s: &str) -> Result<Texto, String> {
        let b = s.as_bytes();
        if b.is_empty() || b.len() > 4 {
            return Err(format!("'{s}': o texto de uma casa tem de 1 a 4 letras"));
        }
        let mut bytes = [0u8; 4];
        bytes[..b.len()].copy_from_slice(b);
        Ok(Texto { bytes, tam: b.len() as u8 })
    }
    pub fn como_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.tam as usize]).unwrap_or("?")
    }
}

impl fmt::Debug for Texto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.como_str())
    }
}

/// Quantos quartos de volta o desenho esta girado, no sentido horario.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rotacao(pub u8);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Adesivo {
    pub simbolo: Simbolo,
    pub cor: crate::calendario::Cor,
    pub rot: Rotacao,
}

impl Default for Adesivo {
    fn default() -> Self {
        Adesivo {
            simbolo: Simbolo::Vazio,
            cor: crate::calendario::Cor::Preto,
            rot: Rotacao(0),
        }
    }
}

/// Como um adesivo se escreve no formato de texto:
///
/// ```text
///   .       vazio
///   17      data
///   24/31   casa dupla
///   'A      letra
///   @Sun    dia da semana
/// ```
///
/// A cor vem depois, entre parenteses, e a rotacao com `>` repetido:
/// `17(v)>>` e um 17 vermelho de cabeca para baixo. Sem marca, preto e em pe.
impl fmt::Display for Adesivo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::calendario::Cor;
        match self.simbolo {
            Simbolo::Vazio => write!(f, ".")?,
            Simbolo::Data(d) => write!(f, "{d}")?,
            Simbolo::Dupla(a, b) => write!(f, "{a}/{b}")?,
            Simbolo::Letra(t) => write!(f, "'{}", t.como_str())?,
            Simbolo::Dia(d) => write!(f, "@{}", crate::calendario::DIAS[d as usize])?,
        }
        match self.cor {
            Cor::Vermelho => write!(f, "(v)")?,
            Cor::Azul => write!(f, "(a)")?,
            Cor::Preto => {}
        }
        for _ in 0..self.rot.0 {
            write!(f, ">")?;
        }
        Ok(())
    }
}

/// Le um adesivo do formato de texto.
pub fn ler_adesivo(txt: &str) -> Result<Adesivo, String> {
    use crate::calendario::{Cor, DIAS};
    let mut s = txt.trim();
    let mut rot = 0u8;
    while let Some(resto) = s.strip_suffix('>') {
        rot += 1;
        s = resto;
    }
    if rot > 3 {
        return Err(format!("'{txt}': rotacao maior que 3 quartos de volta"));
    }
    let mut cor = Cor::Preto;
    for (marca, c) in [("(v)", Cor::Vermelho), ("(a)", Cor::Azul)] {
        if let Some(resto) = s.strip_suffix(marca) {
            cor = c;
            s = resto;
        }
    }
    let simbolo = if s == "." {
        Simbolo::Vazio
    } else if let Some(letra) = s.strip_prefix('\'') {
        Simbolo::Letra(Texto::novo(letra)?)
    } else if let Some(dia) = s.strip_prefix('@') {
        match DIAS.iter().position(|&d| d.eq_ignore_ascii_case(dia)) {
            Some(i) => Simbolo::Dia(i as u8),
            None => return Err(format!("'{txt}': '{dia}' nao e um dia da semana")),
        }
    } else if let Some((a, b)) = s.split_once('/') {
        Simbolo::Dupla(
            a.parse().map_err(|_| format!("'{txt}': '{a}' nao e numero"))?,
            b.parse().map_err(|_| format!("'{txt}': '{b}' nao e numero"))?,
        )
    } else {
        Simbolo::Data(s.parse().map_err(|_| format!("'{txt}': nao entendi"))?)
    };
    Ok(Adesivo { simbolo, cor, rot: Rotacao(rot) })
}

pub const N: usize = 7;
pub const POR_FACE: usize = N * N;
pub const ADESIVOS: usize = 6 * POR_FACE;
pub const FACES: [&str; 6] = ["U", "R", "F", "D", "L", "B"];

/// O cubo inteiro: 294 adesivos, na ordem face-linha-coluna.
#[derive(Clone)]
pub struct EstadoCal(pub Vec<Adesivo>);

impl EstadoCal {
    pub fn vazio() -> Self {
        EstadoCal(vec![Adesivo::default(); ADESIVOS])
    }

    pub fn casa(&self, face: usize, linha: usize, coluna: usize) -> &Adesivo {
        &self.0[face * POR_FACE + linha * N + coluna]
    }

    pub fn poe(&mut self, face: usize, linha: usize, coluna: usize, a: Adesivo) {
        self.0[face * POR_FACE + linha * N + coluna] = a;
    }

    /// Desenha uma face em grade, para conferir contra o cubo na mao.
    pub fn desenhar_face(&self, face: usize) -> String {
        let mut largura = 0;
        let mut celulas = Vec::new();
        for l in 0..N {
            let mut linha = Vec::new();
            for c in 0..N {
                let t = self.casa(face, l, c).to_string();
                largura = largura.max(t.chars().count());
                linha.push(t);
            }
            celulas.push(linha);
        }
        let mut s = format!("face {}\n", FACES[face]);
        for linha in celulas {
            for t in linha {
                s.push_str(&format!("{t:<largura$} ", largura = largura + 1));
            }
            s.push('\n');
        }
        s
    }

    /// Le uma face de um texto com 49 simbolos separados por espaco.
    pub fn ler_face(&mut self, face: usize, txt: &str) -> Result<(), String> {
        let campos: Vec<&str> = txt.split_whitespace().collect();
        if campos.len() != POR_FACE {
            return Err(format!(
                "face {}: esperava {POR_FACE} adesivos, recebi {}",
                FACES[face],
                campos.len()
            ));
        }
        for (i, campo) in campos.iter().enumerate() {
            self.0[face * POR_FACE + i] = ler_adesivo(campo)?;
        }
        Ok(())
    }
}

/// A primeira face lida das fotos e CONFERIDA contra o cubo fisico, em duas
/// passadas: primeiro simbolo e cor, depois a rotacao. So os simbolos e as
/// cores estao validados aqui; as rotacoes ainda nao.
///
/// Tres casas foram corrigidas pela conferencia, e todas pelo mesmo motivo — eu
/// lia o desenho girado como se estivesse em pe:
///   (0,2) e a letra `l`, nao um `1`;
///   (6,4) e (6,5) sao `a` e `n` (de "Jan"), nao `u` e `p` — que e como um `n`
///   e um `a` de cabeca para baixo se parecem.
pub const FACE_LIDA_1: &str = "    23(v)    .     'l    8(a)  16(a)  .     'Ma
    @Sun(v)  @Mon  .     7     15     .     @Sat(a)
    1(v)     2     3     4     5      6     12(v)
    8(v)     9     10    11    12     13    5(v)
    15(v)    16    17    18    19     20    .
    29       22    23    24    25     .     .
    24/31(v) .     .     .     'a     'n    .";

/// Face 2, lida e conferida. Nenhuma correcao desta vez — as duvidas que
/// levantei (dois 29 pretos vizinhos, e um 17 girado) estavam certas. Numeros
/// repetidos lado a lado sao onde a leitura mais escorrega, entao vale sempre
/// perguntar.
pub const FACE_LIDA_2: &str = "    30(a) 21(v) 9(a) 13(a) 15(a) 25    .
    24    18    5    .     .     .     .
    .     19    4    2     3     4     27
    2(v)  20    3    5     10    11    7(v)
    28    21    2    16    17    18    .
    19(v) .     22   8     1     17    26
    29(a) 27    26   29    29    16(v) 31(a)";

/// Face 3, lida e conferida. Duas correcoes, e a primeira e a lição do cubo:
/// eu li `9` onde havia `6`. E exatamente o truque em que o produto se apoia —
/// a fonte foi desenhada para que um 6 de cabeca para baixo VIRE um 9, e e isso
/// que permite seis centros cobrirem sete arranjos de mes. Quem le, cai.
///
/// A outra: `(6,5)` e a letra `t`, que fecha "Oct" com o `Oc` do canto — eu
/// tinha lido como o numero 4 girado.
pub const FACE_LIDA_3: &str = "    27(v) 17(v) 14(v) 27  20(a) 27(a) 'Fe
    25(a) 24    23    22  21    20    .
    18(a) 17    16    15  14    13    .
    11(a) 10    6     8   7     3     .
    2(a)  3     2     1   .     .     31
    22(v) 23    24    25  26    27    26(a)
    'Oc   .     .     30  13(v) 't    29(v)";

/// Face 4, lida e conferida. Uma correcao, e ela veio do MODELO, nao do olho:
/// `(3,6)` e um `4` vermelho, que eu tinha lido como `7`. O traco horizontal
/// do desenho me fez ver um sete europeu.
///
/// Quem achou o erro foi a prova de que o cubo monta qualquer mes: faltava
/// exatamente um adesivo — a borda vermelha do 4 — enquanto todo outro numero
/// tinha a sua. Um erro em 294 adesivos, achado por checagem independente.
///
/// Ela estabelece um fato que importa para o alvo: existem DUAS pecas `24/31`,
/// uma vermelha (a de canto, peca 8) e uma preta (aqui em (5,6)). Faz sentido —
/// a vermelha serve a coluna de domingo e a preta as outras. Mais uma prova de
/// que a cor faz parte da identidade da peca, nao e enfeite.
pub const FACE_LIDA_4: &str = "    'Se   'y  .   .   .   .   'De
    23(a) 21  14  7   .   .   31
    .     .   1   8   15  .   4(a)
    28    8   4   10  11  .   4(v)
    9(v)  15  16  17  18  .   28
    30    .   .   5   12  19  24/31
    28(v) .   30  31  30  'n  .";
// (a linha 3 termina em 4(v), corrigido: era 7(v))

/// Face 5, lida e conferida. Sem correcoes — inclusive o `6` azul de (3,6), que
/// eu marquei como duvida por ja ter caido no truque do 6/9 na face 3.
///
/// Ela carrega quatro dos sete cabecalhos de dia da semana numa coluna so
/// (`Tue` a `Fri`); os outros tres estao na face 1.
pub const FACE_LIDA_5: &str = "    23/30(v) 20(v) 5(a) 26 25 24(a) 26(v)
    28(a)    25    24   23 1  22    'b
    21(a)    14    13   12 11 @Tue  'g
    14(a)    4     5    6  7  @Wed  6(a)
    7(a)     10    1    8  15 @Thu  27
    28       23    16   9  2  @Fri  .
    24(v)    29    .    .  1(a) 'r  'Ap";

/// Face 6, lida e conferida. Sem correcoes.
pub const FACE_LIDA_6: &str = "    'J    'c  10(v) 10(a) .     30    25(v)
    .     .   11    21    20    21    18(v)
    29    .   12    14    13    20    3(a)
    6(v)  .   13    7     6     19    12(a)
    31    .   14    .     .     18    17(a)
    'v    26  19    12    22    .     22(a)
    'No   28  19(a) 3(v)  11(v) 'p    'A";

/// As seis faces, na ordem em que foram lidas e conferidas.
pub const FACES_LIDAS: [&str; 6] = [
    FACE_LIDA_1,
    FACE_LIDA_2,
    FACE_LIDA_3,
    FACE_LIDA_4,
    FACE_LIDA_5,
    FACE_LIDA_6,
];

/// O 6 e o 9 sao a MESMA peca.
///
/// Nao e coincidencia nem economia: a fonte foi desenhada para que o 6 de
/// cabeca para baixo vire um 9, e e isso que permite seis centros cobrirem os
/// sete arranjos de mes possiveis. Sem esse truque o cubo nao existiria.
///
/// Para o solver, a consequencia e direta: quem precisa de um 9 pode usar um 6
/// girado, e vice-versa. Tratar os dois como pecas distintas tornaria alvos
/// perfeitamente montaveis em impossiveis.
///
/// (Foi tambem onde eu mais errei lendo as fotos — duas vezes.)
pub fn mesmo_desenho(a: &Simbolo, b: &Simbolo) -> bool {
    let normal = |s: &Simbolo| match s {
        Simbolo::Data(9) => Simbolo::Data(6),
        // MESMA ideia nas letras, e foi o teste dos meses que revelou: nao
        // existe peca `u` no cubo, e mesmo assim junho, julho e agosto pedem
        // uma. Existe `n` — e `u` de cabeca para baixo E `n`. O fabricante
        // aplicou o truque do 6/9 tambem no alfabeto.
        Simbolo::Letra(t) if t.como_str() == "u" => {
            Simbolo::Letra(Texto::novo("n").unwrap())
        }
        outro => *outro,
    };
    normal(a) == normal(b)
}

/// Os CANTOS ditados com o cubo na mao. Cada canto e uma peca so, com tres
/// adesivos — e e por eles que as seis faces se amarram: saber que `Ma`, `Ap` e
/// `J` sao a MESMA peca ja diz que essas tres faces se encontram num vertice.
/// Seis fotos soltas nao contam isso; os cantos contam.
///
/// Confirma tambem a economia do fabricante nos nomes de mes: `J` serve
/// janeiro, junho e julho; `Ma` serve marco e maio; `Ap` e abril.
/// Os oito, ditados com o cubo na mao. Duas coisas que eles ja provam:
/// - a COR faz parte da identidade da peca — o canto 3 traz dois 29, um azul e
///   um vermelho, e sao adesivos distintos;
/// - o fabricante economiza nos nomes de mes: `J` serve janeiro, junho e julho,
///   `Ma` serve marco e maio.
pub const CANTOS: [[&str; 3]; 8] = [
    ["Ap", "Ma", "J"],
    ["De", "No", "."],
    ["A", "Oc", "."],
    ["29(a)", "29(v)", "28(v)"],
    ["24(v)", "23(v)", "."],
    ["25(v)", "26(v)", "27(v)"],
    ["23/30(v)", "30(a)", "Fe"],
    ["Se", "31(a)", "24/31(v)"],
];

/// Os quatro cantos de cada face, lidos das fotos, na ordem
/// (0,0) (0,6) (6,0) (6,6). `.` e adesivo em branco — e e justamente onde a
/// leitura sozinha nao decide qual peca esta ali.
pub const CANTOS_DAS_FACES: [[&str; 4]; 6] = [
    ["23(v)", "Ma", "24/31(v)", "."],      // a face quase montada
    ["30(a)", ".", "29(a)", "31(a)"],
    ["27(v)", "Fe", "Oc", "29(v)"],
    ["Se", "De", "28(v)", "."],
    ["23/30(v)", "26(v)", "24(v)", "Ap"],
    ["J", "25(v)", "No", "A"],
];

/// Qual peca de canto esta em cada face, deduzido — nao ditado.
///
/// Tres faces mostram um canto em BRANCO, e tres pecas (as que tem um adesivo
/// vazio) precisam de uma terceira face. Isso da seis atribuicoes possiveis, e
/// so UMA sobrevive a regra que define um cubo: duas faces compartilham 0
/// cantos (sao opostas) ou exatamente 2 (sao vizinhas). Nunca 1.
///
/// Foi assim que a estrutura saiu sem refotografar nada — os cantos ditados
/// bastaram.
pub fn deduzir_faces() -> Result<Vec<[usize; 4]>, String> {
    // de cada face, as pecas que a leitura ja identifica (e as casas em branco)
    let mut conhecidas: Vec<Vec<usize>> = Vec::new();
    let mut vagas: Vec<usize> = Vec::new();
    for (fi, face) in CANTOS_DAS_FACES.iter().enumerate() {
        let mut pecas = Vec::new();
        for adesivo in face.iter() {
            if *adesivo == "." {
                vagas.push(fi);
                continue;
            }
            match CANTOS.iter().position(|p| p.contains(adesivo)) {
                Some(i) => pecas.push(i),
                None => return Err(format!("face {fi}: '{adesivo}' nao esta em nenhum canto")),
            }
        }
        conhecidas.push(pecas);
    }
    // as pecas que ainda nao apareceram em tres faces
    let faltando: Vec<usize> = (0..8)
        .filter(|&p| conhecidas.iter().filter(|f| f.contains(&p)).count() < 3)
        .collect();
    if faltando.len() != vagas.len() {
        return Err(format!(
            "{} casas em branco para {} pecas incompletas",
            vagas.len(),
            faltando.len()
        ));
    }
    // testa todas as atribuicoes; guarda as que formam um cubo de verdade
    let mut boas: Vec<Vec<[usize; 4]>> = Vec::new();
    let n = faltando.len();
    let mut ordem: Vec<usize> = (0..n).collect();
    permutacoes(&mut ordem, 0, &mut |perm: &[usize]| {
        let mut faces = conhecidas.clone();
        for (k, &fi) in vagas.iter().enumerate() {
            faces[fi].push(faltando[perm[k]]);
        }
        // regra do cubo: 0 ou 2 cantos em comum, nunca 1 nem 3
        for a in 0..6 {
            for b in (a + 1)..6 {
                let comuns = faces[a].iter().filter(|p| faces[b].contains(p)).count();
                if comuns != 0 && comuns != 2 {
                    return;
                }
            }
        }
        // e cada peca em exatamente tres faces
        for p in 0..8 {
            if faces.iter().filter(|f| f.contains(&p)).count() != 3 {
                return;
            }
        }
        boas.push(
            faces
                .iter()
                .map(|f| [f[0], f[1], f[2], f[3]])
                .collect(),
        );
    });
    match boas.len() {
        1 => Ok(boas.remove(0)),
        0 => Err("nenhuma atribuicao forma um cubo — ha erro na leitura".into()),
        n => Err(format!("{n} atribuicoes possiveis: falta informacao")),
    }
}

fn permutacoes(v: &mut Vec<usize>, k: usize, f: &mut impl FnMut(&[usize])) {
    if k == v.len() {
        f(v);
        return;
    }
    for i in k..v.len() {
        v.swap(k, i);
        permutacoes(v, k + 1, f);
        v.swap(k, i);
    }
}

/// Uma PECA do cubo-calendario: onde ela esta e o que mostra em cada casa.
#[derive(Clone, Debug)]
pub struct Peca {
    /// as casas que ela ocupa (1 para centro, 2 para aresta, 3 para canto)
    pub casas: Vec<usize>,
    /// o desenho de cada casa, na mesma ordem
    pub mostra: Vec<Adesivo>,
}

impl Peca {
    /// Quantos adesivos com desenho a peca tem — os em branco nao contam para
    /// identifica-la.
    pub fn desenhados(&self) -> usize {
        self.mostra.iter().filter(|a| a.simbolo != Simbolo::Vazio).count()
    }
}

/// Agrupa os 294 adesivos nas pecas do cubo. E o passo que separa "mapa de
/// adesivos" de "cubo": tres adesivos de um canto sao UMA peca, e movem juntos.
pub fn inventario(estado: &EstadoCal) -> Vec<Peca> {
    let cn = crate::cuben::cuben(N);
    cn.pecas()
        .into_iter()
        .map(|casas| {
            let mostra = casas.iter().map(|&f| estado.0[f]).collect();
            Peca { casas, mostra }
        })
        .collect()
}

/// Uma face lida, girada `k` quartos de volta no sentido horario.
fn girar(face: &[Adesivo], k: u8) -> Vec<Adesivo> {
    let mut atual = face.to_vec();
    for _ in 0..k {
        let mut novo = vec![Adesivo::default(); POR_FACE];
        for l in 0..N {
            for c in 0..N {
                // girar 90 graus: a casa (l,c) passa a mostrar o que estava em
                // (N-1-c, l)
                novo[l * N + c] = atual[(N - 1 - c) * N + l];
            }
        }
        atual = novo;
    }
    atual
}

/// Monta o cubo com as fotos numa dada ordem e rotacao.
pub fn montar(ordem: &[usize; 6], giros: &[u8; 6]) -> EstadoCal {
    let mut e = EstadoCal::vazio();
    for (destino, (&foto, &k)) in ordem.iter().zip(giros.iter()).enumerate() {
        let mut face = EstadoCal::vazio();
        face.ler_face(0, FACES_LIDAS[foto]).expect("face valida");
        let girada = girar(&face.0[..POR_FACE], k);
        e.0[destino * POR_FACE..(destino + 1) * POR_FACE].copy_from_slice(&girada);
    }
    e
}

/// Descobre COMO as seis fotos se encaixam: qual face do cubo cada uma e, e
/// quantos quartos de volta esta girada.
///
/// Seis fotos soltas nao dizem isso, e sem isso nao ha cubo. Mas os CANTOS
/// ditados dizem: cada um e uma peca com tres adesivos, e so o encaixe certo
/// produz exatamente aqueles oito trios. Sao 720 ordens x 4096 rotacoes — um
/// espaco pequeno, e o criterio e exato.
pub fn descobrir_montagem() -> Vec<([usize; 6], [u8; 6])> {
    let cn = crate::cuben::cuben(N);
    let cantos: Vec<Vec<usize>> = cn.pecas().into_iter().filter(|p| p.len() == 3).collect();
    let esperado = {
        let mut v: Vec<Vec<String>> = CANTOS
            .iter()
            .map(|c| {
                let mut t: Vec<String> = c.iter().map(|s| s.to_string()).collect();
                t.sort();
                t
            })
            .collect();
        v.sort();
        v
    };
    // as seis faces lidas, ja giradas nas quatro posicoes
    let mut giradas: Vec<[Vec<Adesivo>; 4]> = Vec::new();
    for txt in FACES_LIDAS.iter() {
        let mut face = EstadoCal::vazio();
        face.ler_face(0, txt).expect("face valida");
        let base = face.0[..POR_FACE].to_vec();
        giradas.push([
            girar(&base, 0),
            girar(&base, 1),
            girar(&base, 2),
            girar(&base, 3),
        ]);
    }
    let mut achados = Vec::new();
    let mut ordem = [0usize; 6];
    let mut usada = [false; 6];
    permuta_faces(&mut ordem, &mut usada, 0, &mut |ordem: &[usize; 6]| {
        for bits in 0..4096u32 {
            let mut giros = [0u8; 6];
            for (i, g) in giros.iter_mut().enumerate() {
                *g = ((bits >> (2 * i)) & 3) as u8;
            }
            let mut trios: Vec<Vec<String>> = cantos
                .iter()
                .map(|casas| {
                    let mut t: Vec<String> = casas
                        .iter()
                        .map(|&f| {
                            let face = f / POR_FACE;
                            let dentro = f % POR_FACE;
                            let foto = ordem[face];
                            giradas[foto][giros[face] as usize][dentro]
                                .to_string()
                                .trim_start_matches('\'')
                                .to_string()
                        })
                        .collect();
                    t.sort();
                    t
                })
                .collect();
            trios.sort();
            if trios == esperado {
                achados.push((*ordem, giros));
            }
        }
    });
    achados
}

fn permuta_faces(
    ordem: &mut [usize; 6],
    usada: &mut [bool; 6],
    k: usize,
    f: &mut impl FnMut(&[usize; 6]),
) {
    if k == 6 {
        f(ordem);
        return;
    }
    for i in 0..6 {
        if usada[i] {
            continue;
        }
        usada[i] = true;
        ordem[k] = i;
        permuta_faces(ordem, usada, k + 1, f);
        usada[i] = false;
    }
}

/// O cubo montado, com o encaixe descoberto pelos cantos.
///
/// A busca acha 24 montagens compativeis, e esse numero e a prova de que esta
/// certo: um cubo tem exatamente 24 orientacoes no espaco, entao as 24 sao o
/// MESMO cubo visto de angulos diferentes. A montagem e unica a menos de girar
/// o cubo inteiro — que e o melhor resultado possivel, e nao muda nada para
/// resolver.
pub fn cubo_montado() -> Result<EstadoCal, String> {
    let achados = descobrir_montagem();
    match achados.len() {
        0 => Err("nenhuma montagem bate com os cantos ditados".into()),
        24 => {
            let (ordem, giros) = achados[0];
            Ok(montar(&ordem, &giros))
        }
        n => Err(format!("{n} montagens — esperava 24 (as orientacoes de um cubo)")),
    }
}

/// O cubo lido das fotos, montado na ordem em que foram lidas — SEM o encaixe
/// descoberto. Serve para inspecao, nao para resolver.
pub fn cubo_lido() -> EstadoCal {
    let mut e = EstadoCal::vazio();
    for (i, txt) in FACES_LIDAS.iter().enumerate() {
        e.ler_face(i, txt).expect("as faces conferidas sao validas");
    }
    e
}

/// A que ORBITA uma casa pertence. Nao basta separar canto, borda e centro: as
/// orbitas sao fechadas — uma peca de centro da orbita 3 nunca alcanca uma casa
/// da orbita 1, por mais movimentos que se faca. Atribuir sem respeitar isso
/// produz alvos impossiveis que so falham na hora de resolver.
pub fn orbita_da_casa(f: usize) -> usize {
    let cn = crate::cuben::cuben(N);
    for (oi, orb) in cn.orbitas_de_centro().iter().enumerate() {
        if orb.contains(&f) {
            return 100 + oi;
        }
    }
    for (oi, orb) in cn.orbitas_de_asa().iter().enumerate() {
        if orb.iter().any(|w| w.contains(&f)) {
            return 200 + oi;
        }
    }
    if cn.casas_dos_meios().iter().any(|w| w.contains(&f)) {
        return 300;
    }
    if cn.casas_dos_cantos().iter().any(|c| c.contains(&f)) {
        return 400;
    }
    500 // nucleo
}

/// De que tipo e a casa (l,c) de uma face 7x7.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TipoCasa {
    Canto,
    Borda,
    Centro,
}

pub fn tipo_da_casa(l: usize, c: usize) -> TipoCasa {
    let borda_l = l == 0 || l == N - 1;
    let borda_c = c == 0 || c == N - 1;
    match (borda_l, borda_c) {
        (true, true) => TipoCasa::Canto,
        (true, _) | (_, true) => TipoCasa::Borda,
        _ => TipoCasa::Centro,
    }
}

/// O que a face-alvo pede numa casa: um simbolo, com uma cor.
fn pedido(celula: &crate::calendario::Celula, cor: crate::calendario::Cor) -> Adesivo {
    use crate::calendario::Celula;
    let simbolo = match celula {
        Celula::Vazia => Simbolo::Vazio,
        Celula::Data(d) => Simbolo::Data(*d as u8),
        Celula::Dupla(a, b) => Simbolo::Dupla(*a as u8, *b as u8),
        Celula::Letra(t) => Simbolo::Letra(Texto::novo(t).unwrap()),
        Celula::Dia(d) => Simbolo::Dia(
            crate::calendario::DIAS.iter().position(|x| x == d).unwrap_or(0) as u8,
        ),
    };
    Adesivo { simbolo, cor, rot: Rotacao(0) }
}

/// Uma peca SERVE numa casa se tem um adesivo com o desenho e a cor pedidos.
/// O 6 e o 9 contam como o mesmo desenho — a peca so precisa ser girada.
fn serve(peca: &Peca, quer: &Adesivo) -> bool {
    peca.mostra
        .iter()
        .any(|a| mesmo_desenho(&a.simbolo, &quer.simbolo) && a.cor == quer.cor)
}

/// O cubo consegue montar o calendario deste mes?
///
/// Cada uma das 49 casas da face pede um desenho, e so pode ser preenchida por
/// uma peca do TIPO certo — canto no canto, aresta na borda, centro no miolo —
/// que carregue aquele desenho. Cada peca serve no maximo uma casa da face
/// (uma peca so mostra um adesivo por face).
///
/// E um emparelhamento. Se algum mes nao fechar, ou a leitura tem erro ou ha
/// uma restricao do cubo que ainda nao entendemos.
pub fn atribuir(estado: &EstadoCal, face: usize, ano: i32, mes: u32) -> Result<Vec<usize>, String> {
    let pecas = inventario(estado);
    let alvo = crate::calendario::face(ano, mes);
    // casas da face, com o que pedem
    let mut casas: Vec<(usize, usize, Adesivo, TipoCasa)> = Vec::new();
    for l in 0..N {
        for c in 0..N {
            let (celula, cor) = alvo[l][c];
            casas.push((l, c, pedido(&celula, cor), tipo_da_casa(l, c)));
        }
    }
    // Candidatas de cada casa: mesma ORBITA e o desenho pedido. A orbita e o
    // criterio que importa — ela ja distingue canto, borda e centro, e ainda
    // separa as orbitas de centro entre si, que sao fechadas.
    let candidatas: Vec<Vec<usize>> = casas
        .iter()
        .map(|(l, c, quer, _)| {
            let orb = orbita_da_casa(face * POR_FACE + l * N + c);
            pecas
                .iter()
                .enumerate()
                .filter(|(_, p)| orbita_da_casa(p.casas[0]) == orb && serve(p, quer))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    for (i, cands) in candidatas.iter().enumerate() {
        if cands.is_empty() {
            let (l, c, quer, tipo) = &casas[i];
            return Err(format!(
                "({l},{c}) pede {quer} num {tipo:?}, e nenhuma peca serve"
            ));
        }
    }
    // emparelhamento por caminhos aumentantes
    let mut de_peca: Vec<Option<usize>> = vec![None; pecas.len()];
    let mut de_casa: Vec<Option<usize>> = vec![None; casas.len()];
    fn tenta(
        casa: usize,
        candidatas: &[Vec<usize>],
        de_peca: &mut Vec<Option<usize>>,
        de_casa: &mut Vec<Option<usize>>,
        visto: &mut Vec<bool>,
    ) -> bool {
        for &p in &candidatas[casa] {
            if visto[p] {
                continue;
            }
            visto[p] = true;
            let livre = de_peca[p].is_none();
            if livre || tenta(de_peca[p].unwrap(), candidatas, de_peca, de_casa, visto) {
                de_peca[p] = Some(casa);
                de_casa[casa] = Some(p);
                return true;
            }
        }
        false
    }
    for casa in 0..casas.len() {
        let mut visto = vec![false; pecas.len()];
        if !tenta(casa, &candidatas, &mut de_peca, &mut de_casa, &mut visto) {
            let (l, c, quer, _) = &casas[casa];
            return Err(format!("({l},{c}) pede {quer}, mas as pecas ja estao tomadas"));
        }
    }
    Ok(de_casa.into_iter().map(|x| x.unwrap()).collect())
}

/// Qual face usar como alvo para este mes.
///
/// Nao e escolha livre: a casa do meio da face (3,3) e o centro PRESO AO
/// NUCLEO — ele nunca muda de lugar, so gira junto com o cubo inteiro. Entao
/// nao se decide o que fica ali; decide-se por qual face olhar.
///
/// E e nisso que o projeto do cubo se apoia: os seis centros do meio carregam
/// as datas 5 a 11 (sete valores em seis pecas, pelo truque do 6 que vira 9), e
/// a casa (3,3) de um calendario mensal cai justamente nessa faixa. Escolher a
/// face E escolher em que dia da semana o mes comeca.
pub fn escolher_face(estado: &EstadoCal, ano: i32, mes: u32) -> Result<usize, String> {
    let alvo = crate::calendario::face(ano, mes);
    let (celula, cor) = alvo[3][3];
    let quer = pedido(&celula, cor);
    for f in 0..6 {
        let meio = estado.casa(f, 3, 3);
        if mesmo_desenho(&meio.simbolo, &quer.simbolo) && meio.cor == quer.cor {
            return Ok(f);
        }
    }
    let meios: Vec<String> = (0..6).map(|f| estado.casa(f, 3, 3).to_string()).collect();
    Err(format!(
        "nenhuma face tem {quer} no meio (os meios sao {})",
        meios.join(", ")
    ))
}

/// Leva os CENTROS da face-alvo para o lugar.
///
/// Sao 25 das 49 casas, o maior bloco. O tijolo e o 3-ciclo puro: ele move
/// exatamente tres pecas de uma orbita e nada mais, entao as seis orbitas se
/// resolvem em separado, sem uma desfazer a outra.
///
/// A face-alvo e a 0. Como a montagem so e unica a menos de girar o cubo
/// inteiro, qualquer face serve — girar o cubo nao muda o que e possivel.
///
/// So as casas da face importam: as outras 20 de cada orbita ficam como
/// cairem. Foi isso que o experimento mediu — alvo parcial custa ~337
/// movimentos nos centros contra ~1726 do cubo inteiro, e a paridade some,
/// porque as casas que nao importam absorvem a transposicao.
pub fn resolver_centros(estado: &EstadoCal, ano: i32, mes: u32) -> Result<(usize, Vec<usize>), String> {
    let cn = crate::cuben::cuben(N);
    let face_alvo = escolher_face(estado, ano, mes)?;
    let pecas = inventario(estado);
    let atribuicao = atribuir(estado, face_alvo, ano, mes)?;

    let mut movimentos = Vec::new();
    for (oi, orbita) in cn.orbitas_de_centro().iter().enumerate() {
        // onde cada casa da orbita esta, identificada pela casa de origem
        let mut onde: Vec<usize> = orbita.to_vec();

        // o que a face-alvo pede desta orbita: casa -> peca que deve chegar la
        let mut pedidos: Vec<(usize, usize)> = Vec::new();
        for l in 0..N {
            for c in 0..N {
                if tipo_da_casa(l, c) != TipoCasa::Centro {
                    continue;
                }
                let casa = face_alvo * POR_FACE + l * N + c;
                let Some(slot) = orbita.iter().position(|&f| f == casa) else {
                    continue;
                };
                let peca = &pecas[atribuicao[l * N + c]];
                pedidos.push((slot, peca.casas[0]));
            }
        }
        if pedidos.is_empty() {
            continue;
        }

        let aplicar = |onde: &mut Vec<usize>, seq: &[usize]| {
            for &m in seq {
                let cm = cn.permuta_centro(oi, m);
                let antes = onde.clone();
                for (q, &destino) in cm.iter().enumerate() {
                    onde[destino as usize] = antes[q];
                }
            }
        };
        let certas = |onde: &Vec<usize>| {
            pedidos.iter().filter(|(slot, peca)| onde[*slot] == *peca).count()
        };

        let mut guarda = 0;
        while certas(&onde) < pedidos.len() {
            guarda += 1;
            if guarda > 40 {
                return Err(format!(
                    "orbita {oi}: parei em {}/{} casas",
                    certas(&onde),
                    pedidos.len()
                ));
            }
            let base = certas(&onde);
            let mut achou = None;
            'busca: for a in 0..24usize {
                for b in 0..24usize {
                    if b == a {
                        continue;
                    }
                    for c in 0..24usize {
                        if c == a || c == b {
                            continue;
                        }
                        for trio in [[a as u8, b as u8, c as u8], [a as u8, c as u8, b as u8]] {
                            let Some(seq) = cn.ciclo_de_centro(oi, trio) else { continue };
                            let mut teste = onde.clone();
                            aplicar(&mut teste, &seq);
                            if certas(&teste) > base {
                                achou = Some((seq, teste));
                                break 'busca;
                            }
                        }
                    }
                }
            }
            match achou {
                Some((seq, novo)) => {
                    movimentos.extend(seq);
                    onde = novo;
                }
                None => {
                    return Err(format!(
                        "orbita {oi}: nenhum 3-ciclo avanca de {base}/{}",
                        pedidos.len()
                    ))
                }
            }
        }
    }
    Ok((face_alvo, movimentos))
}

/// Os movimentos escritos como um humano leria.
pub fn escrever(movimentos: &[usize]) -> String {
    let cn = crate::cuben::cuben(N);
    movimentos.iter().map(|&m| cn.move_name(m)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendario::Cor;

    #[test]
    fn le_e_escreve_adesivo() {
        for txt in [".", "17", "24/31", "'A", "'Ma", "@Sun", "17(v)", "5(a)>", "23(v)>>", "'M>>>"] {
            let a = ler_adesivo(txt).unwrap_or_else(|e| panic!("{txt}: {e}"));
            assert_eq!(a.to_string(), txt, "ida e volta de '{txt}'");
        }
    }

    #[test]
    fn recusa_o_que_nao_entende() {
        for txt in ["17(x)", "'", "'ABCDE", "@Lun", "a/b", ">>>>", "17>>>>"] {
            assert!(ler_adesivo(txt).is_err(), "'{txt}' deveria ser recusado");
        }
    }

    /// A face conferida com o cubo na mao tem de continuar sendo lida sem erro.
    #[test]
    fn le_a_face_conferida() {
        let mut e = EstadoCal::vazio();
        e.ler_face(0, FACE_LIDA_1).expect("a face conferida deve ser valida");
        assert_eq!(e.casa(0, 1, 0).simbolo, Simbolo::Dia(0), "(1,0) e o cabecalho Sun");
        assert_eq!(e.casa(0, 1, 0).cor, Cor::Vermelho, "domingo e vermelho");
        assert_eq!(e.casa(0, 2, 0).simbolo, Simbolo::Data(1));
        assert_eq!(e.casa(0, 6, 0).simbolo, Simbolo::Dupla(24, 31), "a casa dupla");
        assert_eq!(e.casa(0, 0, 6).simbolo, Simbolo::Letra(Texto::novo("Ma").unwrap()),
            "o canto traz DUAS letras");
        assert_eq!(e.casa(0, 0, 2).simbolo, Simbolo::Letra(Texto::novo("l").unwrap()),
            "(0,2) e a letra l, corrigida na conferencia");
        // o que a foto mostrava como u e p sao a e n girados
        assert_eq!(e.casa(0, 6, 4).simbolo, Simbolo::Letra(Texto::novo("a").unwrap()));
        assert_eq!(e.casa(0, 6, 5).simbolo, Simbolo::Letra(Texto::novo("n").unwrap()));
        // e um retrato do calendario: as datas 1 a 6 em sequencia na linha 2
        for (c, d) in (1..=6u8).enumerate() {
            assert_eq!(e.casa(0, 2, c).simbolo, Simbolo::Data(d));
        }
    }

    /// Os cantos ditados tem de ser legiveis pelo mesmo formato das faces — e
    /// cada um tem exatamente tres adesivos, que e o que define um canto.
    #[test]
    fn le_os_cantos_ditados() {
        for canto in CANTOS.iter() {
            assert_eq!(canto.len(), 3, "canto tem tres adesivos");
            for txt in canto.iter() {
                let t = if txt.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                    format!("'{txt}")
                } else {
                    txt.to_string()
                };
                ler_adesivo(&t).unwrap_or_else(|e| panic!("canto {canto:?}: {e}"));
            }
        }
        assert_eq!(CANTOS.len(), 8, "um cubo tem oito cantos");
        // Os cantos da face ja conferida tem de existir entre as pecas ditadas.
        // E a primeira checagem cruzada entre o que eu li da foto e o que voce
        // ditou do cubo — se um nao existisse, um dos dois estaria errado.
        let mut e = EstadoCal::vazio();
        e.ler_face(0, FACE_LIDA_1).unwrap();
        for (l, c) in [(0, 0), (0, 6), (6, 0), (6, 6)] {
            let mostra = e.casa(0, l, c).to_string();
            let mostra = mostra.trim_start_matches('\'');
            let achou = CANTOS.iter().any(|p| p.iter().any(|a| *a == mostra));
            assert!(achou, "canto ({l},{c}) mostra '{mostra}', que nao esta em nenhuma peca");
        }
    }

    /// A estrutura do cubo sai dos cantos, sem refotografar: a atribuicao das
    /// tres casas em branco tem de ser UNICA. Se este teste passar a acusar
    /// varias solucoes, e sinal de que uma leitura mudou e virou ambigua.
    #[test]
    fn a_estrutura_sai_dos_cantos() {
        let faces = deduzir_faces().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(faces.len(), 6);
        // cada peca em exatamente tres faces
        for p in 0..8 {
            let quantas = faces.iter().filter(|f| f.contains(&p)).count();
            assert_eq!(quantas, 3, "peca {p} em {quantas} faces");
        }
        // tres pares de faces opostas (zero cantos em comum)
        let mut opostas = 0;
        for a in 0..6 {
            for b in (a + 1)..6 {
                let comuns = faces[a].iter().filter(|p| faces[b].contains(p)).count();
                assert!(comuns == 0 || comuns == 2, "faces {a} e {b} tem {comuns} cantos em comum");
                if comuns == 0 {
                    opostas += 1;
                }
            }
        }
        assert_eq!(opostas, 3, "um cubo tem tres pares de faces opostas");
        for (i, f) in faces.iter().enumerate() {
            println!("face {i}: cantos {f:?}");
        }
    }

    /// As faces conferidas continuam validas, e os cantos delas batem com as
    /// pecas ditadas — a checagem cruzada entre o que eu li e o que voce ditou.
    #[test]
    fn as_faces_conferidas_batem_com_os_cantos() {
        for (i, txt) in FACES_LIDAS.iter().enumerate() {
            let mut e = EstadoCal::vazio();
            e.ler_face(0, txt).unwrap_or_else(|erro| panic!("face {i}: {erro}"));
            for (l, c) in [(0, 0), (0, 6), (6, 0), (6, 6)] {
                let mostra = e.casa(0, l, c).to_string();
                let mostra = mostra.trim_start_matches('\'');
                if mostra == "." {
                    continue; // casa em branco: a peca sai da deducao, nao da leitura
                }
                assert!(
                    CANTOS.iter().any(|p| p.contains(&mostra)),
                    "face {i}, canto ({l},{c}) mostra '{mostra}', que nao esta em nenhuma peca"
                );
            }
        }
    }

    /// As seis faces lidas formam um cubo com 294 adesivos, e nenhuma delas
    /// deixou casa por preencher.
    #[test]
    fn as_seis_faces_formam_o_cubo() {
        let mut e = EstadoCal::vazio();
        for (i, txt) in FACES_LIDAS.iter().enumerate() {
            e.ler_face(i, txt).unwrap_or_else(|erro| panic!("face {i}: {erro}"));
        }
        assert_eq!(e.0.len(), ADESIVOS, "294 adesivos");
        let cheias = e.0.iter().filter(|a| a.simbolo != Simbolo::Vazio).count();
        println!("{cheias} adesivos com desenho, {} em branco", ADESIVOS - cheias);
        assert!(cheias > 200, "a maioria das casas traz desenho");
    }

    /// O 6 e o 9 sao a mesma peca — girada.
    #[test]
    fn seis_e_nove_sao_a_mesma_peca() {
        assert!(mesmo_desenho(&Simbolo::Data(6), &Simbolo::Data(9)));
        assert!(mesmo_desenho(&Simbolo::Data(9), &Simbolo::Data(6)));
        assert!(!mesmo_desenho(&Simbolo::Data(6), &Simbolo::Data(8)));
        // e nao vale para os outros: 2 e 5 nao viram um ao outro nesta fonte
        assert!(!mesmo_desenho(&Simbolo::Data(2), &Simbolo::Data(5)));
    }

    /// O inventario tem de bater com a geometria de um 7x7: 8 cantos, 12 meios
    /// de aresta, 48 asas e 150 centros — 218 pecas, 294 adesivos.
    #[test]
    fn o_inventario_bate_com_a_geometria() {
        let pecas = inventario(&cubo_lido());
        let cantos = pecas.iter().filter(|p| p.casas.len() == 3).count();
        let duplas = pecas.iter().filter(|p| p.casas.len() == 2).count();
        let solos = pecas.iter().filter(|p| p.casas.len() == 1).count();
        assert_eq!(cantos, 8, "oito cantos");
        assert_eq!(duplas, 12 + 48, "doze meios de aresta e 48 asas");
        assert_eq!(solos, 150, "150 centros, contando os seis do meio");
        assert_eq!(pecas.len(), 218);
        assert_eq!(pecas.iter().map(|p| p.casas.len()).sum::<usize>(), ADESIVOS);
        // nenhuma casa em duas pecas
        let mut vistas = vec![false; ADESIVOS];
        for p in &pecas {
            for &f in &p.casas {
                assert!(!vistas[f], "casa {f} aparece em duas pecas");
                vistas[f] = true;
            }
        }
        assert!(vistas.iter().all(|&v| v), "toda casa pertence a alguma peca");
    }

    /// O encaixe das seis fotos sai dos cantos ditados: so a montagem certa
    /// produz exatamente aqueles oito trios. Sem isso, seis fotos sao seis
    /// quadrados soltos e qualquer solucao calculada sairia errada.
    #[test]
    #[ignore = "busca o encaixe das seis fotos (720 x 4096 combinacoes)"]
    fn descobre_como_as_fotos_se_encaixam() {
        let achados = descobrir_montagem();
        println!("{} montagens compativeis com os cantos ditados", achados.len());
        for (ordem, giros) in achados.iter().take(4) {
            println!("  fotos {ordem:?} giradas {giros:?}");
        }
        // 24 = as orientacoes de um cubo. Achar exatamente 24 quer dizer que a
        // montagem e UNICA a menos de girar o cubo inteiro — todas as 24 sao o
        // mesmo cubo visto de outro angulo. Menos que isso seria contradicao;
        // mais, seria leitura ambigua.
        assert_eq!(
            achados.len(),
            24,
            "a montagem deveria ser unica a menos de rotacao do cubo"
        );
    }

    /// Os cantos do inventario tem de ser exatamente os oito que voce ditou —
    /// e agora com os tres adesivos juntos, deduzidos da geometria e nao da
    /// leitura de uma face isolada.
    /// O solver dos centros: leva as 25 pecas de centro da face-alvo para o
    /// lugar, e o teste confere o resultado SIMULANDO os movimentos — nao
    /// acredita no solver, refaz a conta.
    #[test]
    #[ignore = "solver: os centros da face-alvo"]
    fn resolve_os_centros_da_face() {
        let estado = cubo_montado().expect("montagem");
        for (ano, mes) in [(2026, 8), (2026, 5), (2027, 2)] {
            let (face_alvo, movs) = resolver_centros(&estado, ano, mes)
                .unwrap_or_else(|e| panic!("{ano}-{mes}: {e}"));
            println!(
                "{ano}-{mes:02}: face {face_alvo}, {} movimentos nos centros",
                movs.len()
            );

            // conferencia: aplica os movimentos ao cubo e olha a face 0
            let cn = crate::cuben::cuben(N);
            let mut estado_final = estado.clone();
            let mut posicao: Vec<usize> = (0..ADESIVOS).collect();
            for &m in &movs {
                let mut alto: Vec<u8> = posicao.iter().map(|&x| (x >> 8) as u8).collect();
                let mut baixo: Vec<u8> = posicao.iter().map(|&x| (x & 255) as u8).collect();
                cn.apply(&mut alto, m);
                cn.apply(&mut baixo, m);
                posicao = alto
                    .iter()
                    .zip(baixo.iter())
                    .map(|(&a, &b)| ((a as usize) << 8) | b as usize)
                    .collect();
            }
            for (i, &origem) in posicao.iter().enumerate() {
                estado_final.0[i] = estado.0[origem];
            }
            let alvo = crate::calendario::face(ano, mes);
            let mut erradas = 0;
            for l in 0..N {
                for c in 0..N {
                    if tipo_da_casa(l, c) != TipoCasa::Centro {
                        continue;
                    }
                    let (celula, cor) = alvo[l][c];
                    let quer = pedido(&celula, cor);
                    let tem = estado_final.casa(face_alvo, l, c);
                    if !mesmo_desenho(&tem.simbolo, &quer.simbolo) || tem.cor != quer.cor {
                        if erradas < 3 {
                            println!("  ({l},{c}): queria {quer}, ficou {tem}");
                        }
                        erradas += 1;
                    }
                }
            }
            assert_eq!(erradas, 0, "{ano}-{mes}: {erradas} centros errados apos os movimentos");
        }
    }

    /// Levanta TODOS os desenhos que faltam, de uma vez — em vez de descobrir um
    /// por mes. Cada buraco e um adesivo que eu li errado ou uma restricao do
    /// cubo que ainda nao entendemos.
    #[test]
    #[ignore = "diagnóstico: que desenhos faltam"]
    fn o_que_falta_para_montar_os_meses() {
        let estado = cubo_montado().expect("montagem");
        let pecas = inventario(&estado);
        let tipo_da_peca = |p: &Peca| match p.casas.len() {
            3 => TipoCasa::Canto,
            2 => TipoCasa::Borda,
            _ => TipoCasa::Centro,
        };
        let mut faltando: Vec<(String, TipoCasa)> = Vec::new();
        for ano in 2024..=2030 {
            for mes in 1..=12u32 {
                let alvo = crate::calendario::face(ano, mes);
                for l in 0..N {
                    for c in 0..N {
                        let (celula, cor) = alvo[l][c];
                        let quer = pedido(&celula, cor);
                        let tipo = tipo_da_casa(l, c);
                        let tem = pecas.iter().any(|p| tipo_da_peca(p) == tipo && serve(p, &quer));
                        let _ = tipo;
                        if !tem {
                            let chave = (quer.to_string(), tipo);
                            if !faltando.contains(&chave) {
                                faltando.push(chave);
                            }
                        }
                    }
                }
            }
        }
        println!("{} desenhos faltando:", faltando.len());
        for (d, t) in &faltando {
            println!("  {d} num {t:?}");
        }
        // e o que EXISTE de cada numero, para ajudar a achar o erro de leitura
        for alvo_num in [4u8, 10, 11] {
            let onde: Vec<String> = pecas
                .iter()
                .filter(|p| p.mostra.iter().any(|a| a.simbolo == Simbolo::Data(alvo_num)))
                .map(|p| {
                    let t = format!("{:?}", tipo_da_peca(p));
                    let cores: Vec<String> = p
                        .mostra
                        .iter()
                        .filter(|a| a.simbolo == Simbolo::Data(alvo_num))
                        .map(|a| format!("{a}"))
                        .collect();
                    format!("{t}:{}", cores.join(","))
                })
                .collect();
            println!("  pecas com {alvo_num}: {}", onde.join("  "));
        }
    }

    /// O cubo tem de conseguir montar QUALQUER mes — e isso e uma prova
    /// independente da leitura. Se um mes nao fechasse, ou eu li algo errado ou
    /// ha uma restricao do cubo que ainda nao entendemos.
    #[test]
    #[ignore = "prova: o cubo monta qualquer mês"]
    fn o_cubo_monta_qualquer_mes() {
        let estado = cubo_montado().expect("montagem");
        let mut falhas = Vec::new();
        let mut testados = 0;
        // 2024 a 2030 cobre os sete comecos de semana em todos os doze meses
        for ano in 2024..=2030 {
            for mes in 1..=12u32 {
                testados += 1;
                let face = match escolher_face(&estado, ano, mes) {
                    Ok(f) => f,
                    Err(e) => {
                        falhas.push(format!("{ano}-{mes:02}: {e}"));
                        continue;
                    }
                };
                if let Err(e) = atribuir(&estado, face, ano, mes) {
                    falhas.push(format!("{ano}-{mes:02}: {e}"));
                }
            }
        }
        println!("{testados} meses testados, {} falharam", falhas.len());
        for f in falhas.iter().take(6) {
            println!("  {f}");
        }
        assert!(falhas.is_empty(), "{} meses nao fecham", falhas.len());
    }

    #[test]
    #[ignore = "depende do encaixe descoberto"]
    fn os_cantos_do_inventario_sao_os_ditados() {
        let pecas = inventario(&cubo_montado().expect("montagem"));
        let mut achados: Vec<Vec<String>> = pecas
            .iter()
            .filter(|p| p.casas.len() == 3)
            .map(|p| {
                let mut v: Vec<String> = p
                    .mostra
                    .iter()
                    .map(|a| a.to_string().trim_start_matches('\'').to_string())
                    .collect();
                v.sort();
                v
            })
            .collect();
        achados.sort();
        let mut esperados: Vec<Vec<String>> = CANTOS
            .iter()
            .map(|c| {
                let mut v: Vec<String> = c.iter().map(|s| s.to_string()).collect();
                v.sort();
                v
            })
            .collect();
        esperados.sort();
        for (a, e) in achados.iter().zip(esperados.iter()) {
            println!("  inventario {a:?}  ditado {e:?}");
        }
        assert_eq!(achados, esperados, "os cantos do inventario nao batem com os ditados");
    }

    #[test]
    fn le_uma_face_inteira() {
        let mut e = EstadoCal::vazio();
        let face: String = std::iter::repeat_n("17(v)>", POR_FACE).collect::<Vec<_>>().join(" ");
        e.ler_face(0, &face).expect("face valida");
        assert_eq!(e.casa(0, 3, 3).simbolo, Simbolo::Data(17));
        assert_eq!(e.casa(0, 6, 6).cor, Cor::Vermelho);
        assert_eq!(e.casa(0, 0, 0).rot, Rotacao(1));
        assert!(e.ler_face(0, "17 17").is_err(), "face incompleta tem de ser recusada");
    }
}
