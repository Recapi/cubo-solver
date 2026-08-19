//! Cubo-calendario 7x7: a face-alvo de um mes.
//!
//! Um mes cabe numa face 7x7 assim:
//!
//! ```text
//!   linha 0   nome do mes, uma letra por casa (colunas 0 a 2)
//!   linha 1   cabecalho: Sun Mon Tue Wed Thu Fri Sat
//!   linhas 2-6   cinco linhas de datas
//! ```
//!
//! Cinco linhas de data, e um mes pode precisar de SEIS semanas. A saida do
//! projeto sao as casas DUPLAS: a sexta semana divide a casa com a quinta, na
//! mesma coluna, e a peca traz os dois numeros separados por uma barra
//! (`24/31`). Foi assim que o fabricante fez caber.
//!
//! A cor vem da coluna, como num calendario japones: domingo em vermelho,
//! sabado em azul, o resto em preto. Isso importa para o solver, porque o 12
//! preto e o 12 vermelho sao PECAS diferentes, com destinos diferentes.
//!
//! A regra foi conferida contra as imagens oficiais do tutorial da tribox, que
//! ficam em `/tutorial/months/{MM}{diaDaSemanaDo1}{dias}.png`.

pub const MESES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const DIAS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// A cor de uma casa vem da coluna: domingo vermelho, sabado azul.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cor {
    Preto,
    Vermelho,
    Azul,
}

impl Cor {
    pub fn da_coluna(c: usize) -> Cor {
        match c {
            0 => Cor::Vermelho,
            6 => Cor::Azul,
            _ => Cor::Preto,
        }
    }
}

/// O que uma casa da face mostra.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Celula {
    Vazia,
    /// uma letra do nome do mes
    Letra(char),
    /// cabecalho do dia da semana
    Dia(&'static str),
    Data(u32),
    /// a casa serve a duas semanas: `24/31`
    Dupla(u32, u32),
}

pub type Face = [[(Celula, Cor); 7]; 7];

pub fn bissexto(ano: i32) -> bool {
    (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0
}

pub fn dias_no_mes(ano: i32, mes: u32) -> u32 {
    match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if bissexto(ano) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Dia da semana do dia 1 (0 = domingo), pelo algoritmo de Sakamoto.
pub fn dia_da_semana_do_primeiro(ano: i32, mes: u32) -> u32 {
    let t = [0i32, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let a = if mes < 3 { ano - 1 } else { ano };
    let d = a + a / 4 - a / 100 + a / 400 + t[(mes - 1) as usize] + 1;
    (d.rem_euclid(7)) as u32
}

/// A face-alvo do mes: o que cada uma das 49 casas precisa mostrar.
pub fn face(ano: i32, mes: u32) -> Face {
    let mut f = [[(Celula::Vazia, Cor::Preto); 7]; 7];

    for (i, ch) in MESES[(mes - 1) as usize].chars().enumerate() {
        f[0][i] = (Celula::Letra(ch), Cor::Preto);
    }
    for (c, nome) in DIAS.iter().enumerate() {
        f[1][c] = (Celula::Dia(nome), Cor::da_coluna(c));
    }

    let inicio = dia_da_semana_do_primeiro(ano, mes);
    let total = dias_no_mes(ano, mes);
    for d in 1..=total {
        let pos = inicio + d - 1;
        let (mut linha, coluna) = (pos / 7, (pos % 7) as usize);
        // a sexta semana nao tem linha propria: divide a casa com a quinta
        if linha >= 5 {
            linha -= 1;
        }
        let casa = &mut f[2 + linha as usize][coluna];
        casa.0 = match casa.0 {
            Celula::Data(antes) => Celula::Dupla(antes, d),
            _ => Celula::Data(d),
        };
        casa.1 = Cor::da_coluna(coluna);
    }
    f
}

/// Desenha a face em texto, para conferir contra a imagem oficial ou contra o
/// cubo na mao. Vermelho vem com `*`, azul com `~`.
pub fn desenhar(ano: i32, mes: u32) -> String {
    let f = face(ano, mes);
    let mut s = format!("{} {}\n", MESES[(mes - 1) as usize], ano);
    for linha in f.iter() {
        for (celula, cor) in linha.iter() {
            let marca = match cor {
                Cor::Vermelho => "*",
                Cor::Azul => "~",
                Cor::Preto => " ",
            };
            let texto = match celula {
                Celula::Vazia => ".".to_string(),
                Celula::Letra(c) => c.to_string(),
                Celula::Dia(d) => d.to_string(),
                Celula::Data(d) => d.to_string(),
                Celula::Dupla(a, b) => format!("{a}/{b}"),
            };
            s.push_str(&format!("{marca}{texto:<6}"));
        }
        s.push('\n');
    }
    s
}

/// O endereco da imagem oficial do mes, no tutorial da tribox. Serve para
/// conferir a nossa face contra a do fabricante.
pub fn url_oficial(ano: i32, mes: u32) -> String {
    format!(
        "https://about.tribox.com/images/products/tribox_Calendar_7x7x7_magnetic/tutorial/months/{:02}{}{:02}.png",
        mes,
        dia_da_semana_do_primeiro(ano, mes),
        dias_no_mes(ano, mes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_os_dias_e_o_dia_da_semana() {
        assert_eq!(dias_no_mes(2026, 2), 28);
        assert_eq!(dias_no_mes(2028, 2), 29, "2028 e bissexto");
        assert_eq!(dias_no_mes(2000, 2), 29, "2000 e bissexto (divisivel por 400)");
        assert_eq!(dias_no_mes(1900, 2), 28, "1900 NAO e bissexto");
        // conferidos contra as imagens que o site serve
        assert_eq!(dia_da_semana_do_primeiro(2026, 8), 6, "1/8/2026 e sabado");
        assert_eq!(dia_da_semana_do_primeiro(2026, 5), 5, "1/5/2026 e sexta");
        assert_eq!(dia_da_semana_do_primeiro(2026, 2), 0, "1/2/2026 e domingo");
    }

    /// Toda data do mes aparece exatamente uma vez, e na coluna do seu dia da
    /// semana. E o teste que garante que a face montada e um calendario de
    /// verdade, nao um arranjo qualquer.
    #[test]
    fn toda_data_aparece_uma_vez_na_coluna_certa() {
        for ano in [1999, 2026, 2028, 2099] {
            for mes in 1..=12u32 {
                let f = face(ano, mes);
                let total = dias_no_mes(ano, mes);
                let inicio = dia_da_semana_do_primeiro(ano, mes);
                let mut vistas = vec![0usize; (total + 1) as usize];
                for (r, linha) in f.iter().enumerate().skip(2) {
                    for (c, (celula, cor)) in linha.iter().enumerate() {
                        let datas: Vec<u32> = match celula {
                            Celula::Data(d) => vec![*d],
                            Celula::Dupla(a, b) => vec![*a, *b],
                            _ => vec![],
                        };
                        for d in datas {
                            vistas[d as usize] += 1;
                            let esperada = ((inicio + d - 1) % 7) as usize;
                            assert_eq!(
                                c, esperada,
                                "{ano}-{mes}: dia {d} na coluna {c}, esperava {esperada}"
                            );
                            assert_eq!(*cor, Cor::da_coluna(c), "{ano}-{mes}: cor do dia {d}");
                            assert!(r >= 2 && r <= 6);
                        }
                    }
                }
                for d in 1..=total {
                    assert_eq!(vistas[d as usize], 1, "{ano}-{mes}: dia {d} apareceu {} vezes", vistas[d as usize]);
                }
            }
        }
    }

    /// Casa dupla so existe quando o mes precisa de seis semanas — e ai ela e
    /// obrigatoria. Se aparecesse sem precisar, a face estaria errada.
    #[test]
    fn casa_dupla_so_quando_precisa_de_seis_semanas() {
        for ano in [2024, 2025, 2026, 2027] {
            for mes in 1..=12u32 {
                let semanas = (dia_da_semana_do_primeiro(ano, mes) + dias_no_mes(ano, mes)).div_ceil(7);
                let duplas: usize = face(ano, mes)
                    .iter()
                    .flatten()
                    .filter(|(c, _)| matches!(c, Celula::Dupla(_, _)))
                    .count();
                if semanas <= 5 {
                    assert_eq!(duplas, 0, "{ano}-{mes} cabe em {semanas} semanas, nao deveria ter dupla");
                } else {
                    let sobra = (dia_da_semana_do_primeiro(ano, mes) + dias_no_mes(ano, mes)) as usize - 35;
                    assert_eq!(duplas, sobra, "{ano}-{mes}: {sobra} dias na sexta semana");
                }
            }
        }
    }

    /// Retrato de alguns meses, para conferir contra a imagem oficial (o
    /// endereco sai junto) ou contra o cubo na mao.
    #[test]
    #[ignore = "diagnóstico: desenha a face-alvo de alguns meses"]
    fn retrato_da_face() {
        for (ano, mes) in [(2026, 8), (2026, 5), (2026, 2), (2027, 1)] {
            println!("{}", desenhar(ano, mes));
            println!("oficial: {}\n", url_oficial(ano, mes));
        }
    }
}
