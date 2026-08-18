# Solver de Cubo Mágico (2×2 a 7×7)

Página web onde você pinta as cores do seu cubo e recebe a solução. Servidor e
algoritmos em Rust, sem dependências de solver externas. Seis tamanhos:

- **3×3** — o carro-chefe: soluções ≤ 20 movimentos, modo ótimo com prova,
  CFOP por etapas, entrada guiada com validação em tempo real;
- **2×2** — solução **sempre ótima** (tabela de Deus completa: 3.674.160
  estados, máximo 11 movimentos, instantâneo);
- **4×4** — resolvido por **redução** (centros → parear arestas → resolver como
  3×3), com as paridades OLL/PLL corrigidas por algoritmos certificados por
  simulação; ~110 movimentos e etapas nomeadas no player;
- **5×5, 6×6 e 7×7** — redução genérica, com o fim de jogo **construído** em vez
  de procurado (ver abaixo). Medido no servidor: ~440 movimentos em ~4 s no 5×5,
  ~1000 em 50–80 s no 6×6, ~1180 em **~9 s** no 7×7 (o 6×6 é o mais lento por
  precisar de mais tentativas de paridade). A interface acompanha por job com
  progresso, já que pode passar de um minuto.

## A ideia que fez os cubos grandes funcionarem

Nos cubos grandes, buscar sequências não escala: o fim de jogo pede um movimento
*cirúrgico* (mexer três peças e mais nada), e busca genérica gasta minutos sem
achar. A saída foi trocar procura por construção, em três passos:

1. **Achar um 3-ciclo puro por órbita.** Um comutador `[W, b]` que permuta
   exatamente 3 peças de uma órbita e deixa todo o resto intacto. Existe para
   todas as órbitas de centros e de asas dos três tamanhos (8 a 12 movimentos).
   O ingrediente que faltava era a **fatia pura** — uma camada isolada, que
   neste conjunto de movimentos é a composição `3Rw·Rw'`; sem ela as órbitas
   internas não têm 3-ciclo puro.
2. **Conjugar para onde se precisa.** Conjugação preserva o tamanho do suporte,
   então `V·C·V⁻¹` também mexe em exatamente 3 peças — só muda *quais*. Uma
   busca em largura sobre trios de casas (13.824 estados, pré-computada por
   órbita) dá o `V` para qualquer trio, instantaneamente.
3. **Subir uma medida monotônica.** A montagem dos centros aumenta "quantos
   centros estão na face certa" a cada passo; com o 3-ciclo construído o
   progresso é garantido, sem platô nem reinício aleatório.

As paridades também são **derivadas, não decoradas**: um par de asas trocado é
uma transposição, logo só uma sequência de sinal **ímpar** naquela órbita a
desfaz. O solver calcula o sinal das permutações e escolhe uma sequência ímpar
na órbita travada e par nas outras (medido: os ímpares são os giros largos, e a
fatia pura é par — é o tipo de detalhe que só o cálculo revela). Ver os testes
`base_3ciclos_*` e `movimentos_impares_das_asas`, que registram esses fatos.

## O que mudou, e quanto rendeu

Cada linha abaixo saiu de **medição**, não de estimativa — e em quase todos os
casos o gargalo não era o que eu supunha. O padrão que se repetiu: instrumentar
primeiro (quem resolve cada passo, e a que custo), depois mexer.

### Cubos grandes: de travado a 7,8 s

| passo | o que estava errado | resultado |
|---|---|---|
| Fatia pura no alfabeto | Sem ela, órbitas internas não têm 3-ciclo puro, e o fim de jogo virava busca cega | Centros do 5×5: de travar indefinidamente para 1,2 s |
| Construção em vez de busca | Conjugar um 3-ciclo base cobre qualquer trio; procurar não escala | 6×6 fechou pela primeira vez; 7×7 em 67 s |
| **Ordem dos degraus** | O 3-ciclo construído vinha *depois* das buscas caras | Centros do 7×7: 67 s → **1,6 s**; 7×7 completo: 80 s → **~9 s** |
| Usar todos os processadores | Teto fixo de 12 threads em cinco pontos do código, numa máquina de 24 lógicos | Metade do processador estava ociosa |

A tabela que expôs o problema de ordem, medindo os centros do 7×7:

| degrau | usos | acertos | tempo |
|---|---|---|---|
| `macro3` | 175 | 10 | **706,7 s** |
| `fatia-encaixa` | 176 | 0 | 27,9 s |
| `macro12` | 355 | 179 | 12,0 s |
| `comutador1` | 176 | 0 | 5,5 s |
| `3-ciclo construído` | 165 | **94** | **0,0 s** |

O degrau que resolvia mais casos custava menos que um milissegundo e estava em
penúltimo lugar; o que quase nunca acertava consumia 706 s. Nas asas a medição
contrariou a intuição oposta: os 304 passos foram **todos** resolvidos pela
construção, e os degraus caros ali nunca entram — são apenas rede de segurança.

### Suíte de testes: 691 s → 109 s

| causa | medida | ganho |
|---|---|---|
| Contenção de threads | `cargo test` roda em paralelo e cada busca abria 12 threads; isolados os testes somavam 34 s, juntos davam 392 s | 2 threads sob `cfg(test)` |
| Busca dos 3-ciclos repetida | 66 s por processo, para um resultado fixo por tamanho | Cache em disco (68 a 222 bytes): **66 s → 1 s** |
| Tabelas de poda | 20 testes construíam do zero | Instância compartilhada |
| Casos redundantes | Cada 4×4 custa ~65 s de solver, e a suíte rodava 8 | Versões exaustivas em `--ignored` |

O cache dos 3-ciclos é **sempre reverificado**: cada sequência prova de novo que
é 3-ciclo puro na órbita certa e identidade nas outras. Cache velho vira
recálculo, nunca bug silencioso.

### Uma otimização que foi medida e revertida

Trocar "aceita o primeiro trio que melhora" por "escolhe o de melhor custo por
peça ganha" parecia certo — é de lá que vem quase todo o comprimento da
solução. Na prática o 7×7 saiu de 6,6 s para **mais de 6 minutos**, e o ganho de
comprimento (779 contra 1023 movimentos) era uma amostra só, dentro da variação
normal entre embaralhamentos. Revertido, com o motivo registrado no código para
não se repetir a tentativa sem medir.

### Pendências honestas

- O **6×6 precisa de várias tentativas de paridade** (4 numa medição), enquanto
  o 5×5 acerta de primeira. Medi o sinal da permutação das asas após os centros
  esperando prever isso, e **não prevê**: as mesmas configurações que falham às
  vezes funcionam. Sem a regra correta, corrigir preventivamente estragaria
  casos bons — então ficou como está, com reinício perturbado.
- As soluções são longas (~780 a 1250 movimentos) contra ~200 de um humano. O
  caminho seria uma tabela de casos de fim de jogo, como os conjuntos de
  algoritmos que cubistas decoram.
- O 4×4 também cabe na construção genérica e resolve em 0,2 a 3,5 s (contra
  ~65 s do solver dedicado), mas com ~400 movimentos em vez de ~110. Como nesse
  tamanho alguém executa a solução na mão, o padrão segue no solver curto; o
  caminho genérico fica coberto pelo teste `cubo4_pela_construcao_generica`.

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

1. **Insira o cubo** — do jeito guiado ou livre:
   - **Preencher guiado**: o app pede um adesivo por vez, face a face; o
     adesivo-alvo fica destacado, a câmera do cubo 3D vira sozinha para a face
     da vez, e a paleta **só libera as cores fisicamente possíveis** naquela
     posição (o servidor analisa peças, orientações e paridades do cubo inteiro
     a cada clique — cor impossível fica bloqueada, então é impossível inserir
     um cubo inválido). Tem desfazer, e clicar num adesivo vazio muda o alvo.
   - **Livre**: escolha uma cor e clique nos quadradinhos (dá para arrastar;
     botão direito apaga). Os centros definem a orientação: segure o cubo com o
     **branco em cima** e o **verde na frente**. Se o seu cubo tem outro esquema
     de cores, pode repintar os centros — o servidor deduz as faces deles.
2. **Resolver por etapas (CFOP)** — o método que humanos usam: **cruz → F2L →
   OLL → PLL**. Escolha a **cor da base** (onde a cruz é feita, embaixo) e a da
   **frente**; o cubo 3D reorienta para essa pegada e cada movimento vem com o
   nome da etapa ("Cruz · movimento 2 de 74"). Cruz e pares de F2L são ótimos
   por etapa (busca com tabelas exatas dos subconjuntos); OLL/PLL usam
   algoritmos reais de speedcubing (Sune, T-perm, …) escolhidos por simulação,
   com fallback de 2 olhadas que garante cobertura de todos os 216 OLLs e 288
   PLLs. Média ~60 movimentos.

3. **Resolver** — a solução aparece em notação padrão. Os movimentos com sublinhado
   roxo são os da fase 2. Quatro modos de busca:
   - **rápido** — primeira solução ≤ 20 (menos de 1 ms);
   - **equilibrado** (padrão) — 60 ms tentando encurtar (média ~18,5);
   - **melhor solução** — usa até 10 s procurando encurtar (média ~18,2, máx. 19);
   - **ótimo (com prova)** — prova matematicamente que não existe solução menor.
     Pode levar minutos; se o tempo acabar, mostra a melhor solução encontrada e
     até onde a prova chegou ("provado ≥ N").
   Em **Avançado** dá para ajustar alvo, máximo, tempo e threads na mão.
4. **Passo a passo** — use os controles (ou ←/→ e barra de espaço) para ver o cubo
   a cada movimento, na planificação e no 3D. Arraste o cubo 3D para girar a câmera.

Atalhos: **Embaralhar** gera uma posição aleatória; **Aplicar sequência** executa
uma notação qualquer (`R U R' U2 F2 ...`) sobre o cubo atual.

### 2×2 e 4×4

O seletor no topo troca o tamanho — a planificação e o cubo 3D se reconstroem.
O **Preencher guiado** com validação em tempo real funciona nos três tamanhos:
no 2×2 valida peças de canto e soma de orientações; no 4×4 valida cantos, asas
(emparelhamento perfeito peça↔encaixe respeitando a ordem mostrada, deduzida da
geometria) e contagem de centros. Endpoints: /api/2/allowed e /api/4/allowed.

**2×2**: os 8 cantos são os mesmos do 3×3, então a álgebra é reaproveitada. Sem
centros, o referencial vem do canto de baixo-trás-esquerda (que fica parado
porque o solver usa só U, R e F). Sobram 7! × 3⁶ = 3.674.160 estados: a tabela
de Deus completa é gerada por BFS no primeiro uso (< 1 s) e toda solução é
**ótima** (máximo 11 movimentos — o número de Deus do 2×2, verificado no boot).

**4×4**: modelo próprio de 96 adesivos com 36 movimentos (18 externos + 18
wide), resolvido por **redução**: (1) centros cor a cor, com tabelas exatas
C(24,4) por face; (2) pareamento das 24 "asas" em 12 arestas — busca gulosa
"qualquer par a mais" + busca dirigida com tabelas exatas de 1 e de 2 pares
(posição + bit de flip relativo, que resolve o clássico fim de jogo das últimas
duas arestas entrelaçadas); (3) o reduzido vira um 3×3 e cai no solver
principal. As **paridades** OLL/PLL são detectadas no mapeamento e corrigidas
por algoritmos clássicos **certificados por simulação** (um candidato só é
usado se, aplicado, preserva centros e pares e alterna exatamente a paridade
que promete). As buscas rodam num estado compacto (24 centros + 24 asas com
bit de ordem, regra verificada contra a planificação real) com a raiz dividida
entre threads. API: `/api/2/{scramble,apply,solve}` e `/api/4/...`.

## Como funciona

Algoritmo de **duas fases do Kociemba** com busca IDA\*:

- **Fase 1** leva o cubo para o subgrupo `G1 = <U, D, R2, L2, F2, B2>` — cantos e
  arestas orientados, e as 4 arestas da fatia do meio dentro da fatia.
  Coordenadas: orientação dos cantos (2187) × orientação das arestas (2048) ×
  posição da fatia (495). A heurística é a **distância exata** dessa fase, lida da
  tabela de simetria (abaixo); sem ela (`--no-bigtable`), cai para o máximo de
  três tabelas menores: `dist(fatia, cantos)`, `dist(fatia, arestas)` e
  `dist(cantos, arestas)`.
- **Fase 2** resolve dentro de `G1`, usando só os 10 movimentos que preservam o
  subgrupo. Coordenadas: permutação dos cantos (40320) × das arestas U/D (40320) ×
  da fatia (24).

As tabelas básicas (~9 MB) são geradas por BFS no boot em ~200 ms.

### Tabela de simetria da fase 1

As 16 simetrias do cubo que preservam o eixo U/D (4 rotações em torno de U ×
meia-volta em torno de F × espelho esquerda-direita) reduzem o espaço
flip × fatia de 1.013.760 para **64.430 classes**. Cruzando com a orientação dos
cantos, uma tabela de 64.430 × 2187 ≈ **140 MB** guarda a distância **exata** da
fase 1 de qualquer um dos 2,2 bilhões de estados — a heurística deixa de
subestimar e o IDA\* quase não visita nó fora do caminho (11× menos nós).

A **fase 2** tem a sua própria tabela de simetria: as 40.320 permutações de canto
reduzem a **2.768 classes**; cruzando com as arestas U/D, 2.768 × 40.320 ≈
**112 MB** guardam a distância mínima (com os 10 movimentos de G1) para resolver
cantos + arestas U/D ignorando a fatia — limite inferior quase exato da fase 2,
usado como terceiro componente do `prun2`. Aqui a conjugação é só de
permutações (troca de índices), então o espelho não complica nada.

Detalhes de implementação da fase 1: a conjugação de cantos é feita no nível da
planificação (imune à aritmética de orientação espelhada, fonte clássica de bug)
e a de arestas por multiplicação direta (mod 2 não sofre com espelho). As
tabelas ficam em cache ao lado do executável (`p1sym.cache`, `p2sym.cache`,
`p15sym.cache`) e são regeradas sozinhas se apagadas. `--no-bigtable` /
`NO_BIGTABLE=1` desligam todas (RAM total com todas: ~1,2 GB).

Um detalhe que importa muito: **não basta parar a fase 1 em 12 movimentos** (a
distância máxima até G1). Uma fase 1 mais longa costuma deixar o cubo numa posição
de G1 bem mais fácil, derrubando o total. A busca vai até 21.

### Tabela X (a heurística do modo ótimo)

A fase 1 refinada com a **identidade** das arestas da fatia: orientações (2187 ×
2048) × posição *ordenada* das 4 arestas da fatia (12·11·10·9 = 11.880). São
**3,3 bilhões de estados** por eixo; as 16 simetrias reduzem (flip × epos) a
1.523.864 classes, e a distância é guardada **mod 3, em 2 bits** (~830 MB — em
1 byte seriam 3,3 GB). O mod 3 basta: cada movimento muda a distância em −1, 0
ou +1, então a diferença mod 3 entre vizinhos dá o delta exato e a busca carrega
a distância exata incrementalmente; a da raiz sai "descendo" pela tabela. A
distância máxima nesse espaço é 13, e a média fica ~1 movimento acima da fase 1
— no IDA\*, +1 de heurística média corta ~13× os nós da iteração final. Gerada
por BFS com fronteira em bitmap em ~30 s na primeira execução (pico ~2 GB de
RAM), cacheada em `p15sym.cache` (~930 MB). `--no-xtable` / `NO_XTABLE=1`
desligam só ela (o modo ótimo cai para a heurística de fase 1).

### Solver ótimo (estilo Korf)

O modo ótimo faz IDA\* no espaço completo dos 18 movimentos, com heurística =
máximo das distâncias exatas da **tabela X nos três eixos** do cubo (cada uma é
um limite inferior da distância real). O two-phase entra primeiro como limite
superior; cada iteração do IDA\* que termina vazia prova "não existe solução com
d movimentos". Quando a prova alcança o tamanho da melhor solução, ela é
**provadamente ótima**. A busca ainda escolhe atacar o cubo ou o seu inverso
(o que tiver heurística maior — d(c) = d(c⁻¹)), divide a raiz em ~4.050
subárvores entre as threads e as ordena por heurística crescente, para a
iteração final encontrar a solução mais cedo. Com o tempo esgotado (ou
cancelado), o resultado informa o limite provado.

### Paralelismo

São 6 variantes da mesma posição — **3 eixos** (a fase 1 é definida em relação ao
eixo U/D; girando o cubo inteiro, o mesmo algoritmo passa a usar R/L ou F/B) ×
**direta ou invertida** (resolver o cubo inverso e ler a sequência de trás para
frente dá outra árvore). Threads além das 6 não repetem árvore: **dividem os
movimentos de raiz** com as da mesma variante. A melhor solução é compartilhada
entre todas e usada para podar as demais.

### E a GPU?

Não usei, de propósito. Esta busca é DFS com muito desvio de fluxo e acessos
aleatórios a tabelas de milhões de entradas — exatamente o padrão em que a GPU vai
mal (divergência de warp + cache miss a cada passo). Na CPU o cubo sai em
milissegundos; uma versão em GPU seria bem mais complexa e mais lenta. O paralelismo
que vale aqui é o de poucas threads atacando variantes diferentes da posição.

## Desempenho

Cubos aleatórios, 12 threads, com a tabela de simetria:

| modo | média | máximo | tempo |
|---|---|---|---|
| rápido (primeira ≤ 20) | 19,81 | 20 | **0,2 ms** |
| equilibrado (60 ms encurtando) | **18,5** | 20 | ~60 ms |
| melhor solução (5 s, alvo 15) | **18,05** | 19 | 5 s |
| ótimo (com prova) | 17-18 | — | segundos a minutos |

O esforço mínimo do modo equilibrado existe para a busca não parar na primeira
solução: um cubo a 3 movimentos do fim receberia uma "solução" de 20 movimentos —
a primeira que aparece. Se a posição for fácil, a resposta sai curta e imediata.

## API

Tudo JSON. A planificação é uma string de 54 caracteres, na ordem
`U R F D L B`, cada face lida em linhas (esquerda→direita, cima→baixo).
Qualquer conjunto de 6 caracteres distintos serve — as faces são deduzidas dos centros.

| Rota | Corpo | Resposta |
|---|---|---|
| `POST /api/solve` | `{facelets, max_len?, target_len?, timeout_ms?, min_ms?, threads?}` | `{solution[], notation, length, phase1, phase2, time_ms, nodes, solutions, threads, states[]}` |
| `POST /api/scramble` | `{length?}` | `{facelets, scramble[], notation}` |
| `POST /api/apply` | `{moves, facelets?}` | `{facelets, moves[]}` |
| `POST /api/allowed` | `{facelets (parcial, '.' = vazio), pos}` | `{colors[]}` — cores que mantêm o cubo completável |
| `POST /api/cfop` | `{facelets, base?, front?}` | `{stages[], stage_of[], hold, states[], length}` — solução por etapas |
| `GET /api/health` | — | `ok` |

Parâmetros do `solve`: `max_len` (1–30, padrão 20) é o tamanho máximo aceitável;
`target_len` (padrão = `max_len`) faz a busca parar assim que acha algo com até
esse tamanho — **0 = nunca parar cedo**, usar o tempo todo encurtando;
`timeout_ms` (50–600000; padrão 4000, ou 60000 no modo ótimo) limita a busca;
`min_ms` (padrão 60) é o esforço mínimo antes de aceitar parar no alvo;
`threads` (1–12, padrão: núcleos); `optimal: true` liga o modo ótimo (exige a
tabela de simetria). `solutions` na resposta diz quantas soluções completas
foram encontradas. No modo ótimo a resposta traz também `optimal` (a prova
fechou?) e `lower_bound` (provado que não existe solução menor que isso).

O modo ótimo também roda como **job com progresso**: `POST /api/optimal/start`
`{facelets, timeout_ms?, threads?}` → `{job}`; `GET /api/optimal/status/{job}` →
`{done, elapsed_ms, lower_bound, best_len, nodes, result?}` (a página consulta a
cada 700 ms e mostra "provando ≥ N" ao vivo); `POST /api/optimal/cancel/{job}`
cancela — a resposta vem com a melhor solução e o limite provado até ali.

`states` traz a planificação depois de cada movimento (`length + 1` entradas), que é
o que alimenta o passo a passo da página.

Estados impossíveis são recusados com a explicação do problema — canto torcido,
aresta invertida, paridade inválida, contagem de cores errada, peça repetida.

## Linha de comando

```powershell
.\target\release\cubo-solver.exe --bench 500                 # estatisticas
.\target\release\cubo-solver.exe --bench-optimal 3           # provas de otimalidade
.\target\release\cubo-solver.exe --scramble "R U R' U'"      # sequencia -> planificacao
.\target\release\cubo-solver.exe --solve "<54 caracteres>"   # resolve e imprime
```

O benchmark aceita `BENCH_TARGET`, `BENCH_MAX`, `BENCH_TIMEOUT` (ms), `BENCH_MIN`
(ms), `BENCH_THREADS`, `BENCH_VERBOSE=1` e `BENCH_SEED` (fixa a sequência de
cubos — essencial para comparações A/B). Ex.: `BENCH_MIN=0` mede o tempo até a
primeira solução ≤ alvo; `BENCH_TARGET=15 BENCH_TIMEOUT=10000` mede o modo
"melhor solução".

## Estrutura

```
src/
  cube.rs     estado "cubie", os 18 movimentos, inverso, validacao de paridade
  coord.rs    conversoes estado <-> coordenadas numericas
  facelet.rs  planificacao <-> estado, validacao, rotacoes do cubo inteiro
  tables.rs   tabelas de movimento e de poda (BFS paralelo no boot)
  partial.rs  analise de planificacoes parciais (quais cores podem entrar onde)
  cfop.rs     solver por etapas (cruz, F2L, OLL, PLL) com algoritmos reais
  cube2.rs    2x2: tabela de Deus completa, solucao sempre otima
  cube4.rs    4x4: reducao (centros, pareamento, paridades certificadas)
  sym.rs      16 simetrias do eixo U/D + tabelas das fases 1 e 2 (cache)
  xtable.rs   tabela X do modo otimo (3,3 bilhoes de estados, mod 3 em 2 bits)
  search.rs   IDA* das duas fases, multi-thread
  optimal.rs  solver otimo (IDA* de 3 eixos) com prova, progresso e cancelamento
  main.rs     servidor axum, API JSON, jobs do modo otimo, CLI, testes
static/       pagina, estilo e script (embutidos no binario)
```

## Testes

```powershell
$env:PATH = "C:\msys64\ucrt64\bin;$env:PATH"; cargo test --release
```

Cobrem: ida e volta de todas as coordenadas, os movimentos terem ordem 4, rotações
do cubo terem ordem 3, planificação ↔ estado, recusa de estados impossíveis,
resolução de cubos aleatórios em ≤20 movimentos e o **superflip** (a posição que
exige exatamente 20). Para a tabela de simetria: as 16 simetrias formam grupo
fechado, a conjugação de movimentos bate (espelho leva R em L'), os dois caminhos
de conjugação (planificação × multiplicação de arestas) coincidem, a contagem de
classes dá exatamente 64.430, e a tabela é completa, consistente entre vizinhos e
domina a heurística antiga.
