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
