# Guia do projeto

Solver de cubo mágico (2×2 a 7×7) com servidor e algoritmos em Rust, front-end
em HTML/CSS/JS embutido no binário (`include_str!`). Este arquivo é o que
alguém precisa saber antes de mexer aqui — o README explica o que o projeto
faz; este guia explica como trabalhar nele sem repetir os erros já cometidos.

## Rodar e testar

```powershell
.\iniciar.bat          # compila, sobe em http://localhost:8080 e abre o navegador
.\iniciar.bat 3000     # outra porta
cargo test --release   # suíte padrão: 42 testes, ~113 s
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

## O que está em aberto

- **O 6×6 é hoje o mais caro, e o custo tem nome: recomeço.** Medido em 6 casos
  (`diagnostico_6x6_pipeline`), o tempo vai quase todo em refazer o pipeline
  inteiro quando a paridade das asas exige uma sequência de sinal ímpar — que
  mexe nos centros. Um caso chegou a 8 tentativas, cada uma remontando centros
  (~3 s) e reagrupando as 12 arestas. Os casos que fecham na primeira tentativa
  levam ~3,5 s; os que recomeçam, 15 s ou mais. O caminho é consertar a
  paridade **no lugar** (como já se faz com a sequência certificada quando tudo
  está agrupado) em vez de descartar o trabalho.
- Não adianta trocar a correção de paridade por uma "mais correta": a que vira
  a aresta inteira (largura 3 no 6×6, coerente nas duas órbitas) foi medida e
  custa 96 s onde a atual custa 38 s. Com 11 pares formados quem destrava o
  agrupamento é a *perturbação*, não a troca de paridade — e a coerente não
  perturba nada. Detalhes no comentário do `flip_alg` em `cuben.rs`.
- As soluções são longas (~440 no 5×5, ~1200 no 7×7) contra ~200 de um humano.
  O custo está no método: cada peça de centro consome um comutador de 8 a 12
  movimentos mais a conjugação. Encurtar pede colocar mais peças por comutador.
- O 4×4 dedicado leva ~65 s por cubo (busca IDA* até profundidade 13 nos
  centros). O caminho genérico resolve o mesmo em segundos, com solução mais
  longa; trazer o 3-ciclo construído para dentro do `cube4.rs` daria os dois.
