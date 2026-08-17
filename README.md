# Solver de Cubo 3×3

Página web onde você pinta as cores do seu cubo e recebe a solução em **até 20 movimentos**.
Servidor e algoritmo em Rust, sem dependências de solver externas.

## Rodar

```powershell
.\run.ps1          # sobe em http://localhost:8080
.\run.ps1 3000     # outra porta
```

> **Por que o script?** O toolchain Rust GNU instalado nesta máquina não traz o
> `dlltool.exe` (usado pelo crate `windows-sys`). O `run.ps1` coloca a cópia do
> MSYS2 (`C:\msys64\ucrt64\bin`) no PATH antes de compilar. Se um dia você
> instalar o workload C++ do Visual Studio, dá para trocar o `rust-toolchain.toml`
> para `stable-x86_64-pc-windows-msvc` e dispensar isso.

O HTML/CSS/JS ficam embutidos no binário (`include_str!`), então
`target\release\cubo-solver.exe` roda sozinho, sem precisar da pasta `static\`.
Para mexer no front-end, edite `static\` e recompile.

## Usar

1. **Pinte o cubo** — escolha uma cor e clique nos quadradinhos (dá para arrastar;
   botão direito apaga). Os centros definem a orientação: segure o cubo com o
   **branco em cima** e o **verde na frente**.
   Se o seu cubo tem outro esquema de cores, pode repintar os centros também —
   o servidor deduz as faces a partir deles.
2. **Resolver** — a solução aparece em notação padrão. Os movimentos com sublinhado
   roxo são os da fase 2.
3. **Passo a passo** — use os controles (ou ←/→ e barra de espaço) para ver o cubo
   a cada movimento, na planificação e no 3D. Arraste o cubo 3D para girar a câmera.

Atalhos: **Embaralhar** gera uma posição aleatória; **Aplicar sequência** executa
uma notação qualquer (`R U R' U2 F2 ...`) sobre o cubo atual.

## Como funciona

Algoritmo de **duas fases do Kociemba** com busca IDA\*:

- **Fase 1** leva o cubo para o subgrupo `G1 = <U, D, R2, L2, F2, B2>` — cantos e
  arestas orientados, e as 4 arestas da fatia do meio dentro da fatia.
  Coordenadas: orientação dos cantos (2187) × orientação das arestas (2048) ×
  posição da fatia (495).
- **Fase 2** resolve dentro de `G1`, usando só os 10 movimentos que preservam o
  subgrupo. Coordenadas: permutação dos cantos (40320) × das arestas U/D (40320) ×
  da fatia (24).

As tabelas de poda (distância exata até o objetivo de cada fase, ~4 MB no total)
são geradas por BFS no boot, em paralelo — leva ~60 ms.

Um detalhe que importa muito: **não basta parar a fase 1 em 12 movimentos** (a
distância máxima até G1). Uma fase 1 mais longa costuma deixar o cubo numa posição
de G1 bem mais fácil, derrubando o total. A busca vai até 21.

### Paralelismo

Cada thread ataca uma variante diferente da mesma posição:

- **3 eixos** — a fase 1 é definida em relação ao eixo U/D; girando o cubo inteiro,
  o mesmo algoritmo passa a usar o eixo R/L ou F/B. São três buscas genuinamente
  diferentes. Foi o que mais ajudou: derrubou o tempo médio de 34 ms para 5 ms.
- **direta ou invertida** — resolver o cubo inverso e ler a sequência de trás para
  frente dá outra árvore de busca.
- **ordem das faces** na DFS.

A melhor solução é compartilhada entre as threads e usada para podar as demais.

### E a GPU?

Não usei, de propósito. Esta busca é DFS com muito desvio de fluxo e acessos
aleatórios a tabelas de milhões de entradas — exatamente o padrão em que a GPU vai
mal (divergência de warp + cache miss a cada passo). Na CPU o cubo sai em
milissegundos; uma versão em GPU seria bem mais complexa e mais lenta. O paralelismo
que vale aqui é o de poucas threads atacando variantes diferentes da posição.

## Desempenho

1000 cubos aleatórios (12 threads):

```
media  : 19,03 movimentos
maximo : 20 movimentos
tempo  : ~60 ms por cubo
```

Os 60 ms são o **esforço mínimo** configurado: a busca acha uma solução de ≤20 em
poucos milissegundos e usa o resto do tempo tentando encurtar. Sem isso, um cubo a
3 movimentos do fim receberia uma "solução" de 20 movimentos — a primeira que
aparece. Se a posição for fácil, a resposta sai curta e imediata.

## API

Tudo JSON. A planificação é uma string de 54 caracteres, na ordem
`U R F D L B`, cada face lida em linhas (esquerda→direita, cima→baixo).
Qualquer conjunto de 6 caracteres distintos serve — as faces são deduzidas dos centros.

| Rota | Corpo | Resposta |
|---|---|---|
| `POST /api/solve` | `{facelets, max_len?, timeout_ms?}` | `{solution[], notation, length, phase1, phase2, time_ms, nodes, threads, states[]}` |
| `POST /api/scramble` | `{length?}` | `{facelets, scramble[], notation}` |
| `POST /api/apply` | `{moves, facelets?}` | `{facelets, moves[]}` |
| `GET /api/health` | — | `ok` |

`states` traz a planificação depois de cada movimento (`length + 1` entradas), que é
o que alimenta o passo a passo da página.

Estados impossíveis são recusados com a explicação do problema — canto torcido,
aresta invertida, paridade inválida, contagem de cores errada, peça repetida.

## Linha de comando

```powershell
.\target\release\cubo-solver.exe --bench 500                 # estatisticas
.\target\release\cubo-solver.exe --scramble "R U R' U'"      # sequencia -> planificacao
.\target\release\cubo-solver.exe --solve "<54 caracteres>"   # resolve e imprime
```

`BENCH_TIMEOUT` (ms) e `BENCH_VERBOSE=1` ajustam o benchmark.

## Estrutura

```
src/
  cube.rs     estado "cubie", os 18 movimentos, inverso, validacao de paridade
  coord.rs    conversoes estado <-> coordenadas numericas
  facelet.rs  planificacao <-> estado, validacao, rotacoes do cubo inteiro
  tables.rs   tabelas de movimento e de poda (BFS paralelo no boot)
  search.rs   IDA* das duas fases, multi-thread
  main.rs     servidor axum, API JSON, modos de linha de comando, testes
static/       pagina, estilo e script (embutidos no binario)
```

## Testes

```powershell
$env:PATH = "C:\msys64\ucrt64\bin;$env:PATH"; cargo test --release
```

Cobrem: ida e volta de todas as coordenadas, os movimentos terem ordem 4, rotações
do cubo terem ordem 3, planificação ↔ estado, recusa de estados impossíveis,
resolução de cubos aleatórios em ≤20 movimentos e o **superflip** (a posição que
exige exatamente 20).
