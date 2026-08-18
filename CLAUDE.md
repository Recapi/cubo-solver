# Guia do projeto

Solver de cubo mágico (2×2 a 7×7) com servidor e algoritmos em Rust, front-end
em HTML/CSS/JS embutido no binário (`include_str!`). Hoje resolve, em média:
5×5 em ~300 movimentos e 0,5 s; 6×6 em ~640 e 2,5 s; 7×7 em ~920 e 3,5 s.

Este arquivo é o que alguém precisa saber antes de mexer aqui — o README explica o que o projeto
faz; este guia explica como trabalhar nele sem repetir os erros já cometidos.

## Rodar e testar

```powershell
.\iniciar.bat          # compila, sobe em http://localhost:8080 e abre o navegador
.\iniciar.bat 3000     # outra porta
cargo test --release   # suíte padrão: 46 testes, ~75 s
cargo test --release -- --ignored   # pesados: reduções exaustivas, diagnósticos, régua
```

**Encerre o servidor antes de compilar.** O linker não consegue regravar um
`.exe` em uso e o erro que aparece ("Access is denied") não diz isso. Para
rodar dois testes ao mesmo tempo, aponte `CARGO_TARGET_DIR` para outra pasta
(ex.: `target-alt`, que está no `.gitignore`) — senão os links colidem.

O `dlltool.exe` não vem no toolchain Rust GNU desta máquina; os dois scripts de
inicialização põem `C:\msys64\ucrt64\bin` no PATH antes de compilar.

## Como mexer sem quebrar

**Meça antes e depois, no mesmo caso.** A régua existe para isso:

```powershell
cargo test --release regua_cubos_grandes -- --ignored --nocapture
```

Ela usa embaralhamentos de semente fixa, então compara o mesmo cubo. Com
embaralhamento aleatório a variação entre casos esconde ou inventa ganhos —
foi assim que uma "melhoria" que levava o 7×7 de 6,6 s para mais de 6 minutos
quase entrou como boa. Para saber *onde* o tempo vai, `CUBEN_DEBUG=1` imprime
linhas `DEGRAU` (asas) e `CDEGRAU` (centros) com quem resolveu e quanto custou;
tabular isso já apontou um degrau que gastava 706 s para acertar 10 de 175
tentativas enquanto outro resolvia 94 casos em tempo desprezível.

**Fixe as threads antes de comparar.** As buscas param no primeiro achado, e
quem acha primeiro depende de qual thread chegou antes — o solver é
não-determinístico. Medido: o mesmo binário, mesma semente, resolveu um 6×6 em
926 movimentos/3,3 s numa rodada e 956/10,3 s na seguinte. Para medir, use
`CUBEN_WORKERS=1` (uma thread, resultado reprodutível); em produção deixe sem a
variável, que aí usa todos os processadores. Resta um resíduo de ±2 movimentos
vindo do 3×3 final: o Kociemba melhora a solução até estourar um orçamento de
tempo (`timeout_ms` em `search.rs`), então o polimento depende do relógio. Para
comparar variantes isso é ruído desprezível; só não trate contagem exata como
prova de igualdade.

**Toda busca cara precisa de teto.** Sem limite de candidatos/profundidade, o
solver gasta minutos onde um caminho simples resolve em segundos — e, medido, o
teto melhorou também a *consistência*, não só a média.

**Não edite arquivo-fonte pelo PowerShell.** `Set-Content` e `-replace`
corromperam os acentos do `app.js` de forma irreversível (`—`, `×`, `✓` viraram
`?`). Use as ferramentas de edição; se não houver jeito, escreva com
`[IO.File]::WriteAllText(..., UTF8Encoding($false))` e confira com
`node --check` ou `cargo check`.

**Prefira construção a busca.** O padrão que fez os cubos grandes funcionarem é
conjugar um 3-ciclo puro para o trio que se quer arrumar (ver README). Busca
genérica entra só como rede de segurança, depois dos degraus construtivos.

## Mapa do código

| arquivo | o que faz |
|---|---|
| `search.rs`, `coord.rs`, `tables.rs`, `sym.rs`, `xtable.rs` | 3×3: Kociemba em duas fases, coordenadas, tabelas de poda e simetrias |
| `optimal.rs` | 3×3 ótimo com prova (limite inferior por iteração) |
| `cfop.rs` | 3×3 por etapas: cruz, F2L, OLL, PLL |
| `cube2.rs` | 2×2 com tabela de Deus completa (sempre ótimo) |
| `cube4.rs` | 4×4 dedicado: redução com paridades certificadas por simulação |
| `cuben.rs` | 5×5, 6×6 e 7×7 genéricos (e o 4×4 também cabe): 3-ciclos construídos |
| `simplify.rs` | limpeza da sequência final, usada por todos os solvers |
| `partial.rs` | quais cores ainda cabem numa casa (preenchimento guiado) |
| `main.rs` | servidor axum, endpoints e a suíte de testes |

## Paridades: corrigidas no lugar, nunca refazendo

Um cubo grande esbarra em três paridades, e todas se resolvem **no lugar** —
nenhuma delas refaz o cubo. Antes, cada uma custava remontar centros e as 12
arestas; hoje custa de 7 a 15 movimentos.

| paridade | quando | correção |
|---|---|---|
| OLL do mapa 3×3 (aresta virada) | cubos pares | `oll_alg`, 15 mov |
| PLL do mapa 3×3 (duas peças trocadas) | cubos pares | `pll_alg`, 7 mov |
| aresta travada no agrupamento | ímpares (e par na órbita interna) | `wing_swap_alg`, 15 mov, conjugada até a aresta |

Todas são **certificadas por simulação no build**, e o critério é o mesmo: os
centros ficam intactos e as órbitas continuam agrupadas. Para as do 3×3, quem
julga é o próprio `to_cubie` que recusaria a redução — aplicada ao cubo
resolvido, a sequência tem de produzir exatamente o erro que corrige (paridade
é invariante de dois estados, então o que cria o erro num cubo bom o cancela
num cubo ruim).

Duas armadilhas já pagas, registradas nos comentários: exigir que a correção
mantenha as **peças do meio** fixas barra justamente o algoritmo que funciona
(os meios podem permutar, desde que a aresta continue coerente); e a largura do
algoritmo decide **qual órbita** ele afeta, por isso a certificação é por
órbita.

## O que está em aberto

- **As soluções ainda são longas** (~300 no 5×5, ~630 no 6×6, ~900 no 7×7)
  contra ~200/400/600 de um humano. O raio-X (`raio_x_do_comprimento`) mostra
  a divisão por fase; as linhas `CGASTO`/`WGASTO` (`CUBEN_DEBUG=1`) dizem quem
  produz os movimentos e a que custo por peça/par. Hoje o gargalo é o 3-ciclo
  cirúrgico das asas: faz 126 dos 168 pares a **16,7 movimentos por par**,
  contra ~10 de um humano. O próximo salto pede outro método — freeslice de
  verdade, várias arestas por par de fatias —, não mais ajuste no que existe.
- Os centros do 7×7 são ~50% do comprimento, a 5,8 mov/peça. A cadeia fechada
  (3 peças por ciclo) já está lá; passar disso pede construção de barras.
- O 4×4 dedicado leva ~65 s por cubo (busca IDA* até profundidade 13 nos
  centros). O caminho genérico resolve o mesmo em segundos, com solução mais
  longa; trazer o 3-ciclo construído para dentro do `cube4.rs` daria os dois.
