//! Limpeza da sequencia final: tira movimento redundante.
//!
//! Duas regras, e a segunda e a que rende de verdade:
//!
//! 1. movimentos seguidos na MESMA camada viram um so (`U U` -> `U2`,
//!    `U U'` -> nada, `U2 U2` -> nada);
//! 2. movimentos do MESMO EIXO comutam (girar R nao interfere em L), entao
//!    podem ser reordenados ate se encontrarem: `R L R'` -> `L`.
//!
//! Sem a regra 2 sobra muita coisa, porque o solver alterna camadas do mesmo
//! eixo o tempo todo ao montar centros e arestas.

/// `layer(m)` identifica a camada (mesma camada = da para juntar),
/// `axis(m)` o eixo (mesmo eixo = comuta), e `build(camada, potencia)` monta
/// o movimento de volta, com potencia 0 = um quarto, 1 = meia volta, 2 = um
/// quarto ao contrario.
pub fn simplify<L, A, B>(moves: &[usize], layer: L, axis: A, build: B) -> Vec<usize>
where
    L: Fn(usize) -> usize,
    A: Fn(usize) -> usize,
    B: Fn(usize, usize) -> usize,
{
    let com: Vec<(usize, usize)> = moves.iter().map(|&m| (m, 0)).collect();
    simplify_com_rotulos(&com, layer, axis, build).into_iter().map(|(m, _)| m).collect()
}

/// Igual a `simplify`, mas cada movimento carrega um rotulo (ex.: o indice da
/// etapa a que pertence) que sobrevive a limpeza. Quando dois movimentos se
/// fundem, fica o rotulo do que JA estava na saida — o mais antigo.
///
/// Existe porque mapear a lista limpa de volta as etapas pelo INDICE nao
/// funciona: apos o primeiro cancelamento tudo desliza, e a etapa final
/// ("Resolver como 3x3") chegava a exibir 0 movimentos na interface.
pub fn simplify_com_rotulos<L, A, B>(
    moves: &[(usize, usize)],
    layer: L,
    axis: A,
    build: B,
) -> Vec<(usize, usize)>
where
    L: Fn(usize) -> usize,
    A: Fn(usize) -> usize,
    B: Fn(usize, usize) -> usize,
{
    let quartos = |m: usize| match m % 3 {
        0 => 1u32,
        1 => 2,
        _ => 3,
    };
    let mut atual: Vec<(usize, usize)> = moves.to_vec();
    loop {
        let mut saida: Vec<(usize, usize)> = Vec::with_capacity(atual.len());
        let mut mudou = false;
        for &(m, rot) in &atual {
            // procura para tras, atravessando so o que comuta com m
            let mut i = saida.len();
            let mut alvo = None;
            while i > 0 {
                let ant = saida[i - 1].0;
                if layer(ant) == layer(m) {
                    alvo = Some(i - 1);
                    break;
                }
                if axis(ant) != axis(m) {
                    break; // nao comuta: nao da para passar por cima
                }
                i -= 1;
            }
            match alvo {
                Some(k) => {
                    mudou = true;
                    let total = (quartos(saida[k].0) + quartos(m)) % 4;
                    if total == 0 {
                        saida.remove(k); // um desfaz o outro
                    } else {
                        let pot = match total {
                            1 => 0,
                            2 => 1,
                            _ => 2,
                        };
                        saida[k].0 = build(layer(m), pot);
                    }
                }
                None => saida.push((m, rot)),
            }
        }
        atual = saida;
        if !mudou {
            return atual;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::simplify;

    // encoding simples de 3x3: camada = face, eixo = face % 3
    fn s(ms: &[usize]) -> Vec<usize> {
        simplify(ms, |m| m / 3, |m| (m / 3) % 3, |c, p| c * 3 + p)
    }
    const U: usize = 0; // U
    const U2: usize = 1;
    const UL: usize = 2; // U'
    const R: usize = 3;
    const D: usize = 9;
    const L: usize = 12;

    #[test]
    fn junta_e_cancela_na_mesma_camada() {
        assert_eq!(s(&[U, U]), vec![U2], "U U deveria virar U2");
        assert_eq!(s(&[U, UL]), Vec::<usize>::new(), "U U' deveria sumir");
        assert_eq!(s(&[U2, U2]), Vec::<usize>::new(), "U2 U2 deveria sumir");
        assert_eq!(s(&[U, U, U]), vec![UL], "tres quartos = um ao contrario");
    }

    #[test]
    fn atravessa_o_que_comuta() {
        // R e L sao do mesmo eixo: comutam, entao os dois R se encontram
        assert_eq!(s(&[R, L, R]), vec![R + 1, L], "R L R deveria virar R2 L");
        // U e D comutam; o U' cancela o U do inicio
        assert_eq!(s(&[U, D, UL]), vec![D], "U D U' deveria virar D");
    }

    #[test]
    fn nao_atravessa_eixo_diferente() {
        // R nao comuta com U: nada a juntar
        assert_eq!(s(&[U, R, U]), vec![U, R, U]);
    }
}
