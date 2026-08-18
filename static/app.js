(function () {
  "use strict";

  var FACES = ["U", "R", "F", "D", "L", "B"];
  var FACE_LABEL = { U: "cima", R: "direita", F: "frente", D: "baixo", L: "esquerda", B: "trás" };
  // [posicao da face na frase, cor no esquema padrao]
  var FACE_PT = {
    U: ["de cima", "branca"],
    R: ["da direita", "vermelha"],
    F: ["da frente", "verde"],
    D: ["de baixo", "amarela"],
    L: ["da esquerda", "laranja"],
    B: ["de trás", "azul"],
  };
  // Posicao (linha, coluna) de cada face na planificacao 12x9
  var NET_POS = { U: [0, 3], L: [3, 0], F: [3, 3], R: [3, 6], B: [3, 9], D: [6, 3] };

  var SOLVED = "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB";

  // tamanho do cubo (2, 3 ou 4); o 3x3 tem todos os recursos
  var N = 3;
  function perFace() { return N * N; }
  function totalStickers() { return 6 * N * N; }
  function solvedString() {
    var out = "";
    FACES.forEach(function (f) { for (var i = 0; i < N * N; i++) out += f; });
    return out;
  }
  /** Centro fixo do meio da face: existe só nos cubos ímpares e define o
   *  esquema de cores, então vem pré-preenchido e é pulado no guiado. */
  function isCenter(i) {
    if (N % 2 === 0) return false;
    return i % (N * N) === (N * N - 1) / 2;
  }

  /** Preenche os centros fixos (ímpares) com a cor canônica da face. */
  function fillFixedCenters() {
    if (N % 2 === 0) return;
    var meio = (N * N - 1) / 2;
    for (var f = 0; f < 6; f++) state[f * N * N + meio] = FACES[f];
  }

  // Eixo e sentido da rotacao de camada de cada face (sinal = horario olhando
  // para a face, nas coordenadas 3D do CSS, onde o eixo y aponta para baixo).
  var TURN = {
    U: ["rotateY", -90],
    D: ["rotateY", 90],
    R: ["rotateX", 90],
    L: ["rotateX", -90],
    F: ["rotateZ", 90],
    B: ["rotateZ", -90],
  };

  // --------------------------------------------------------------- estado
  // `state` e o cubo que o usuario montou (a fonte da verdade para resolver).
  // `shown`  e o que esta desenhado na tela: igual a `state`, exceto enquanto
  // o usuario percorre a solucao passo a passo.
  var state = SOLVED.split("");
  var shown = SOLVED;
  var selected = "U";
  var painting = false;

  var solution = null;
  var step = 0; // quantos movimentos ja foram aplicados na visualizacao
  var timer = null;
  var anim = null; // animacao de camada em andamento: {t: timeout, done: fn}

  var netCells = [];
  var cubeCells = []; // indice do adesivo -> plano 3d do cubinho
  var cubies = []; // {el, x, y, z}
  var orientTo = null; // vira a camera para uma face (so no modo guiado)
  var guided = null; // { stack: [pos...], target: pos } quando o modo guiado esta ativo

  var $ = function (id) { return document.getElementById(id); };

  /** "R" -> gire um quarto de volta no horario; "R'" -> anti-horario; "R2" -> 180. */
  function moveDesc(name) {
    var camadas = 2;
    if (/^\d/.test(name)) { camadas = +name[0]; name = name.slice(1); }
    if (name.indexOf("w") > 0) {
      var g = FACE_PT[name[0]];
      var sufixo = name.slice(-1) === "'" ? "no sentido anti-horário"
        : name.slice(-1) === "2" ? "meia-volta" : "no sentido horário";
      var qtd = camadas === 2 ? "as DUAS camadas" : "as " + camadas + " camadas";
      return "Gire " + qtd + " " + g[0] + " (" + g[1] + ") juntas, " + sufixo + ".";
    }
    var f = FACE_PT[name[0]];
    var base = "Gire a face " + f[0] + " (" + f[1] + ") ";
    if (name.length > 1 && name[1] === "2") return base + "meia-volta — 180°, tanto faz o sentido.";
    if (name.length > 1 && name[1] === "'") return base + "um quarto de volta no sentido anti-horário.";
    return base + "um quarto de volta no sentido horário.";
  }

  // --------------------------------------------------------------- montagem
  function buildPalette() {
    var p = $("palette");
    FACES.forEach(function (f) {
      var b = document.createElement("div");
      b.className = "swatch c-" + f + (f === selected ? " sel" : "");
      b.dataset.face = f;
      var tecla = FACES.indexOf(f) + 1;
      b.title = "cor da face " + FACE_LABEL[f] + " (tecla " + tecla + ")";
      b.innerHTML = "<b class=\"tecla\">" + tecla + "</b><span>" + FACE_LABEL[f] + "</span>";
      b.addEventListener("click", function () {
        if (guided) {
          // no modo guiado, clicar numa cor liberada pinta o adesivo-alvo
          if (!b.classList.contains("off")) guidedPaint(f);
          return;
        }
        selected = f;
        Array.prototype.forEach.call(p.children, function (c) {
          c.classList.toggle("sel", c.dataset.face === f);
        });
      });
      p.appendChild(b);
    });
  }

  function buildNet() {
    var net = $("net");
    net.innerHTML = "";
    netCells = [];
    net.style.gridTemplateColumns = "repeat(" + 4 * N + ", var(--cell))";
    net.style.gridTemplateRows = "repeat(" + 3 * N + ", var(--cell))";
    var cellPx = { 2: "38px", 3: "30px", 4: "23px", 5: "19px", 6: "16px", 7: "14px" };
    net.style.setProperty("--cell", cellPx[N] || "30px");
    var off = { U: [0, N], L: [N, 0], F: [N, N], R: [N, 2 * N], B: [N, 3 * N], D: [2 * N, N] };
    FACES.forEach(function (f, fi) {
      var pos = off[f];
      for (var k = 0; k < N * N; k++) {
        var d = document.createElement("div");
        d.className = "st";
        d.style.gridRow = pos[0] + Math.floor(k / N) + 1;
        d.style.gridColumn = pos[1] + (k % N) + 1;
        d.dataset.i = fi * N * N + k;
        if (isCenter(fi * N * N + k)) d.classList.add("center");
        netCells[fi * N * N + k] = d;
        net.appendChild(d);
      }
    });
  }

  function bindNetEvents() {
    var net = $("net");
    net.addEventListener("pointerdown", function (e) {
      var t = e.target.closest(".st");
      if (!t) return;
      e.preventDefault();
      var i = +t.dataset.i;
      if (guided) {
        // no guiado, clicar num adesivo vazio escolhe ele como alvo
        if (!isCenter(i) && FACES.indexOf(state[i]) < 0) setTarget(i);
        return;
      }
      painting = true;
      paint(i, selected);
    });
    net.addEventListener("pointermove", function (e) {
      if (!painting || guided) return;
      var el = document.elementFromPoint(e.clientX, e.clientY);
      if (el && el.classList.contains("st")) paint(+el.dataset.i, selected);
    });
    window.addEventListener("pointerup", function () { painting = false; });
    net.addEventListener("contextmenu", function (e) {
      var t = e.target.closest(".st");
      if (!t) return;
      e.preventDefault();
      if (!guided) paint(+t.dataset.i, ".");
    });
  }

  var CELL_ROT = {
    U: "rotateX(90deg)", D: "rotateX(-90deg)", F: "", B: "rotateY(180deg)",
    R: "rotateY(90deg)", L: "rotateY(-90deg)",
  };

  function buildCube3d() {
    var c = $("cube3d");
    c.innerHTML = "";
    cubies = [];
    cubeCells = [];
    var cell = 132 / N;
    var off = (132 - cell) / 2;
    var half = (N - 1) / 2;

    // N^3 - interior cubinhos, cada um com 6 planos (internos = "plastico")
    var byPos = {};
    for (var x = 0; x < N; x++) {
      for (var y = 0; y < N; y++) {
        for (var z = 0; z < N; z++) {
          var interior = x > 0 && x < N - 1 && y > 0 && y < N - 1 && z > 0 && z < N - 1;
          if (interior) continue;
          var el = document.createElement("div");
          el.className = "cubie";
          el.style.left = off + "px";
          el.style.top = off + "px";
          el.style.width = cell + "px";
          el.style.height = cell + "px";
          var t = "translate3d(" + (x - half) * cell + "px," + (y - half) * cell + "px," + (z - half) * cell + "px)";
          el.dataset.t = t;
          el.style.transform = t;
          var planes = {};
          FACES.forEach(function (p) {
            var s = document.createElement("div");
            s.className = "stk";
            s.dataset.base = "stk";
            s.style.transform = CELL_ROT[p] + " translateZ(" + cell / 2 + "px)";
            planes[p] = s;
            el.appendChild(s);
          });
          byPos[x + "," + y + "," + z] = planes;
          cubies.push({ el: el, x: x, y: y, z: z });
          c.appendChild(el);
        }
      }
    }

    // Liga cada adesivo da planificacao ao plano 3d correspondente.
    var M = N - 1;
    FACES.forEach(function (f, fi) {
      for (var k = 0; k < N * N; k++) {
        var r = Math.floor(k / N), col = k % N, x, y, z;
        if (f === "U") { x = col; y = 0; z = r; }
        else if (f === "D") { x = col; y = M; z = M - r; }
        else if (f === "F") { x = col; y = r; z = M; }
        else if (f === "B") { x = M - col; y = r; z = 0; }
        else if (f === "R") { x = M; y = r; z = M - col; }
        else { x = 0; y = r; z = col; } // L
        cubeCells[fi * N * N + k] = byPos[x + "," + y + "," + z][f];
      }
    });
  }

  function bindCameraEvents() {
    var c = $("cube3d");
    // camera arrastavel; no modo guiado ela tambem vira para a face da vez
    var rx = -24, ry = -34, drag = null, animT = null;
    var scene = $("scene");
    var applyCam = function () {
      c.style.transform = "rotateX(" + rx + "deg) rotateY(" + ry + "deg)";
    };
    scene.addEventListener("pointerdown", function (e) {
      drag = { x: e.clientX, y: e.clientY, rx: rx, ry: ry };
      c.classList.remove("anim");
      scene.classList.add("dragging");
      scene.setPointerCapture(e.pointerId);
    });
    scene.addEventListener("pointermove", function (e) {
      if (!drag) return;
      ry = drag.ry + (e.clientX - drag.x) * 0.5;
      rx = Math.max(-89, Math.min(89, drag.rx - (e.clientY - drag.y) * 0.5));
      applyCam();
    });
    var end = function () { drag = null; scene.classList.remove("dragging"); };
    scene.addEventListener("pointerup", end);
    scene.addEventListener("pointercancel", end);

    var view = { U: [-62, -34], D: [42, -34], F: [-18, -24], B: [-18, 152], R: [-18, -66], L: [-18, 62] };
    orientTo = function (face) {
      if (drag) return;
      var t = view[face];
      if (!t) return;
      var dy = ((t[1] - ry) % 360 + 540) % 360 - 180; // caminho mais curto
      rx = t[0];
      ry = ry + dy;
      c.classList.add("anim");
      applyCam();
      clearTimeout(animT);
      animT = setTimeout(function () { c.classList.remove("anim"); }, 500);
    };
  }

  // --------------------------------------------------------------- animacao de camada
  /** Cubinhos das `espessura` camadas a partir de `face` (1 = só a externa). */
  function layerOf(face, espessura) {
    var e = espessura || 1;
    var M = N - 1;
    return cubies.filter(function (cb) {
      if (face === "U") return cb.y < e;
      if (face === "D") return cb.y > M - e;
      if (face === "R") return cb.x > M - e;
      if (face === "L") return cb.x < e;
      if (face === "F") return cb.z > M - e;
      return cb.z < e; // B
    });
  }

  /** "R" -> 1 camada, "Rw" -> 2, "3Rw" -> 3. */
  function thicknessOf(name) {
    if (/^\d/.test(name)) return +name[0];
    return name.indexOf("w") > 0 ? 2 : 1;
  }

  /** Tira o prefixo numérico: "3Rw2" -> "Rw2". */
  function baseName(name) {
    return /^\d/.test(name) ? name.slice(1) : name;
  }

  function resetCubies() {
    cubies.forEach(function (cb) {
      cb.el.style.transition = "none";
      cb.el.style.transform = cb.el.dataset.t;
    });
  }

  /** Termina a animacao em andamento aplicando o resultado dela. */
  function finishAnim() {
    if (!anim) return;
    clearTimeout(anim.t);
    var d = anim.done;
    anim = null;
    resetCubies();
    d();
  }

  /** Descarta a animacao em andamento (usado antes de pulos arbitrarios). */
  function abortAnim() {
    if (!anim) return;
    clearTimeout(anim.t);
    anim = null;
    resetCubies();
  }

  /** Gira a camada do movimento `name` na tela e chama `done` ao terminar. */
  /** Ritmo do player: soluções longas (cubos grandes) tocam mais rápido —
   *  1200 movimentos a 1s cada seriam 20 minutos. */
  function passoMs() {
    var n = solution ? solution.length : 0;
    if (n > 600) return 260;
    if (n > 200) return 400;
    if (n > 60) return 650;
    return 1000;
  }

  function animateMove(name, done) {
    var espessura = thicknessOf(name);
    var nome = baseName(name); // sem o prefixo numérico
    var sufixo = nome.slice(nome.indexOf("w") > 0 ? 2 : 1); // "", "'" ou "2"
    var t = TURN[nome[0]];
    var deg = t[1] * (sufixo === "'" ? -1 : sufixo === "2" ? 2 : 1);
    var teto = Math.max(120, passoMs() - 70); // a animação cabe no intervalo
    var dur = Math.min(sufixo === "2" ? 520 : 340, teto);
    var layer = layerOf(nome[0], espessura);
    resetCubies();
    // Comeca de "rotacao 0 graus" do MESMO eixo: com as listas de transform
    // identicas, o navegador interpola o angulo e a peca descreve um arco.
    // Sem isso ele interpola as matrizes e a peca corta caminho em linha reta
    // (nos movimentos de 180 graus ela atravessava o meio do cubo).
    layer.forEach(function (cb) {
      cb.el.style.transform = t[0] + "(0deg) " + cb.el.dataset.t;
    });
    void $("cube3d").offsetWidth; // forca o navegador a aplicar o estado inicial
    layer.forEach(function (cb) {
      cb.el.style.transition = "transform " + dur + "ms cubic-bezier(.4,.1,.2,1)";
      cb.el.style.transform = t[0] + "(" + deg + "deg) " + cb.el.dataset.t;
    });
    anim = {
      done: done,
      t: setTimeout(function () {
        anim = null;
        resetCubies();
        done();
      }, dur + 40),
    };
  }

  // --------------------------------------------------------------- desenho
  function cls(ch) { return "c-" + (FACES.indexOf(ch) >= 0 ? ch : "none"); }

  /** Desenha a planificacao e o cubo 3d (qualquer tamanho). */
  function draw(str) {
    var prev = shown;
    shown = str;
    var n = totalStickers();
    for (var i = 0; i < n; i++) {
      var k = cls(str[i]);
      var cell = netCells[i];
      cell.className = "st" + (isCenter(i) ? " center " : " ") + k;
      if (prev.length === n && prev[i] !== str[i]) {
        cell.classList.add("changed");
        (function (c) { setTimeout(function () { c.classList.remove("changed"); }, 420); })(cell);
      }
      var s = cubeCells[i];
      s.className = s.dataset.base + " " + k;
    }
  }

  /** Volta a mostrar o cubo do usuario. */
  function refresh() { draw(state.join("")); }

  function missingCount() {
    return state.filter(function (c) { return FACES.indexOf(c) < 0; }).length;
  }

  function complete() { return missingCount() === 0; }

  function say(msg, kind) {
    var el = $("status");
    el.textContent = msg;
    el.className = "status" + (kind ? " " + kind : "");
  }

  /** Descarta a solucao atual (qualquer mudanca no cubo a invalida). */
  function resetSolution() {
    stopPlay();
    abortAnim();
    stopOptimalJob(true); // mexer no cubo cancela uma prova em andamento
    solution = null;
    step = 0;
    $("moves").innerHTML = "";
    $("result").innerHTML = "";
    $("player").classList.add("hidden");
    $("move-now").classList.add("hidden");
  }

  /** Chamado depois de qualquer alteracao em `state`. */
  function stateChanged(msg, kind) {
    if (guided) exitGuided(); // acoes externas (embaralhar etc.) saem do guiado
    resetSolution();
    refresh();
    if (msg !== undefined) {
      say(msg, kind);
      return;
    }
    var n = missingCount();
    say(n > 0 ? "Faltam " + n + " adesivo" + (n > 1 ? "s" : "") + "." : "");
  }

  function paint(i, color) {
    if (state[i] === color) return;
    state[i] = color;
    stateChanged();
  }

  // ------------------------------------------------------------ modo guiado
  // Preenche face a face: o adesivo-alvo fica destacado, a camera vira para a
  // face da vez e a paleta so libera as cores que mantem o cubo possivel.
  var FACE_FILL_ORDER = ["U", "F", "R", "B", "L", "D"];

  function guidedPositions() {
    var out = [];
    var pf = perFace();
    FACE_FILL_ORDER.forEach(function (f) {
      var fi = FACES.indexOf(f);
      for (var k = 0; k < pf; k++) {
        var i = fi * pf + k;
        if (!isCenter(i)) out.push(i);
      }
    });
    return out;
  }

  function clearTargetMark() {
    for (var i = 0; i < totalStickers(); i++) {
      netCells[i].classList.remove("target");
      cubeCells[i].classList.remove("tgt3");
    }
  }

  function setPaletteAllowed(colors) { // null = modo livre, tudo liberado
    Array.prototype.forEach.call($("palette").children, function (el) {
      var off = colors !== null && colors.indexOf(el.dataset.face) < 0;
      el.classList.toggle("off", off);
    });
  }

  function partialString() {
    return state.map(function (c) { return FACES.indexOf(c) >= 0 ? c : "."; }).join("");
  }

  function enterGuided() {
    stateChanged(); // limpa solucao; a pintura ja feita e mantida
    if (complete()) {
      // cubo ja todo pintado: o guiado e para inserir um novo, comeca limpo
      state = ".".repeat(totalStickers()).split("");
      fillFixedCenters();
      refresh();
    }
    guided = { stack: [], target: -1 };
    $("btn-guided").classList.add("hidden");
    $("guided-bar").classList.remove("hidden");
    nextTarget();
  }

  function exitGuided(msg, kind) {
    if (!guided) return;
    guided = null;
    clearTargetMark();
    setPaletteAllowed(null);
    $("btn-guided").classList.remove("hidden");
    $("guided-bar").classList.add("hidden");
    if (msg !== undefined) say(msg, kind || "");
  }

  function nextTarget() {
    var order = guidedPositions();
    for (var i = 0; i < order.length; i++) {
      if (FACES.indexOf(state[order[i]]) < 0) {
        setTarget(order[i]);
        return;
      }
    }
    exitGuided("Cubo completo e válido — pode resolver!", "ok");
  }

  function setTarget(pos) {
    guided.target = pos;
    clearTargetMark();
    netCells[pos].classList.add("target");
    cubeCells[pos].classList.add("tgt3");
    // a face vem do tamanho do cubo (era fixo em 9: no 4x4 a camera virava
    // para a face errada no meio do preenchimento)
    if (orientTo) orientTo(FACES[Math.floor(pos / perFace())]);
    var faltam = state.filter(function (c) { return FACES.indexOf(c) < 0; }).length;
    say("Pinte o adesivo destacado — cores impossíveis ficam bloqueadas (faltam " + faltam + ").");
    setPaletteAllowed([]); // trava enquanto consulta o servidor
    var url = N === 3 ? "/api/allowed" : "/api/" + N + "/allowed";
    api(url, { facelets: partialString(), pos: pos })
      .then(function (j) {
        if (!guided || guided.target !== pos) return;
        if (j.colors.length === 0) {
          // pintura previa (feita no modo livre) ja era impossivel
          say("O que já estava pintado é impossível — recomeçando do zero.", "err");
          state = ".".repeat(totalStickers()).split("");
          fillFixedCenters();
          guided.stack = [];
          refresh();
          nextTarget();
          return;
        }
        setPaletteAllowed(j.colors);
      })
      .catch(function (e) {
        exitGuided(e.message, "err");
      });
  }

  function guidedPaint(color) {
    if (!guided || guided.target < 0) return;
    var pos = guided.target;
    state[pos] = color;
    guided.stack.push(pos);
    resetSolution();
    refresh();
    nextTarget();
  }

  function guidedUndo() {
    if (!guided || guided.stack.length === 0) return;
    var pos = guided.stack.pop();
    state[pos] = ".";
    refresh();
    setTarget(pos);
  }

  // --------------------------------------------------------------- api
  function api(path, body) {
    return fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body || {}),
    }).then(function (r) {
      return r.json().then(function (j) {
        if (!r.ok) throw new Error(j.error || ("erro " + r.status));
        return j;
      });
    });
  }

  // --------------------------------------------------------------- acoes
  function doScramble() {
    var url = N === 3 ? "/api/scramble" : "/api/" + N + "/scramble";
    api(url, { length: 25 })
      .then(function (j) {
        state = j.facelets.split("");
        stateChanged(j.notation ? "Embaralhado com: " + j.notation : "Embaralhado.", "");
      })
      .catch(function (e) { say(e.message, "err"); });
  }

  function doApply() {
    var seq = $("seq").value.trim();
    if (!seq) return;
    if (N !== 3) {
      if (!complete()) {
        say("Pinte o cubo inteiro antes de aplicar uma sequência.", "err");
        return;
      }
      api("/api/" + N + "/apply", { facelets: state.join(""), moves: seq })
        .then(function (j) {
          state = j.facelets.split("");
          stateChanged("Aplicado: " + seq, "ok");
          $("seq").value = "";
        })
        .catch(function (e) { say(e.message, "err"); });
      return;
    }
    var body = { moves: seq };
    if (complete()) body.facelets = state.join("");
    api("/api/apply", body)
      .then(function (j) {
        state = j.facelets.split("");
        stateChanged("Aplicado: " + j.moves.join(" "), "ok");
        $("seq").value = "";
      })
      .catch(function (e) { say(e.message, "err"); });
  }

  // ------------------------------------------------------- 2x2 e 4x4
  function doSolveOther() {
    var n = missingCount();
    if (n > 0) {
      say("Faltam " + n + " adesivo" + (n > 1 ? "s" : "") + " para poder resolver.", "err");
      return;
    }
    var btn = $("btn-solve");
    btn.disabled = true;
    btn.textContent = "Resolvendo...";
    resetSolution();
    refresh();
    say("");
    // 5x5+ leva minutos: job com progresso em vez de espera cega
    var pedido = N >= 5
      ? api("/api/" + N + "/solve", { facelets: state.join("") }).then(seguirJobGrande)
      : api("/api/" + N + "/solve", { facelets: state.join("") });
    pedido
      .then(function (j) {
        solution = {
          solution: j.solution,
          states: j.states,
          length: j.length,
          stageOf: j.stage_of || null,
          stages: j.stages || null,
        };
        if (N === 2) {
          $("result").innerHTML =
            "<b>" + j.length + " movimentos</b> &middot; " +
            "<b class=\"opt-ok\">ÓTIMO — o mínimo possível</b> &middot; " + j.time_ms + " ms";
          renderChipsSimple(j.solution);
        } else {
          renderStagesResult(j, null);
        }
        $("player").classList.remove("hidden");
        $("move-now").classList.remove("hidden");
        $("p-range").max = j.length;
        $("p-range").value = 0;
        say("Solução pronta. Use o player passo a passo.", "ok");
        jump(0);
      })
      .catch(function (e) {
        resetSolution();
        refresh();
        say(e.message, "err");
      })
      .finally(function () {
        btn.disabled = false;
        btn.textContent = solveLabel();
      });
  }

  /** Acompanha um job de cubo grande ate o fim, mostrando a etapa atual. */
  function seguirJobGrande(inicio) {
    var id = inicio.job;
    return new Promise(function (resolve, reject) {
      var tick = function () {
        fetch("/api/big/status/" + id)
          .then(function (r) { return r.json(); })
          .then(function (s) {
            if (s.error) { reject(new Error(s.error)); return; }
            if (s.done) {
              if (!s.result) { reject(new Error("job sem resultado")); return; }
              resolve(s.result);
              return;
            }
            var seg = Math.round((s.elapsed_ms || 0) / 1000);
            say(
              (s.stage || "resolvendo") + "… " + (s.moves || 0) +
              " movimentos até agora (" + seg + "s)",
              ""
            );
            setTimeout(tick, 700);
          })
          .catch(reject);
      };
      tick();
    });
  }

  // Lista de movimentos: com 1183 fichas o navegador congelava só de montar o
  // DOM. Guardamos a receita e desenhamos uma janela ao redor do movimento
  // atual, e só quando a lista está aberta.
  var listaSpec = null; // { nomes: [], classe: fn(i) -> string, titulo: fn(i) }
  var listaJanela = { de: -1, ate: -1 };
  var JANELA = 150;

  function definirLista(nomes, classe, titulo) {
    listaSpec = { nomes: nomes, classe: classe, titulo: titulo };
    listaJanela = { de: -1, ate: -1 };
    marcarLista(nomes.length);
    desenharLista();
  }

  function desenharLista() {
    var box = $("moves");
    if (!listaSpec || !$("lista-wrap").open) { box.innerHTML = ""; return; }
    var n = listaSpec.nomes.length;
    var de = Math.max(0, Math.min(step - JANELA / 3, n - JANELA) | 0);
    if (de < 0) de = 0;
    var ate = Math.min(n, de + JANELA);
    if (de === listaJanela.de && ate === listaJanela.ate) return; // já desenhada
    listaJanela = { de: de, ate: ate };
    box.innerHTML = "";
    if (de > 0) box.appendChild(avisoLista("… " + de + " movimentos antes"));
    for (var i = de; i < ate; i++) {
      var b = document.createElement("button");
      b.className = "mv " + listaSpec.classe(i);
      b.textContent = listaSpec.nomes[i];
      b.title = listaSpec.titulo(i);
      (function (k) {
        b.addEventListener("click", function () { stopPlay(); jump(k); });
      })(i);
      box.appendChild(b);
    }
    if (ate < n) box.appendChild(avisoLista("… mais " + (n - ate)));
  }

  function avisoLista(txt) {
    var s = document.createElement("span");
    s.className = "mv-aviso";
    s.textContent = txt;
    return s;
  }

  function renderChipsSimple(names) {
    definirLista(names, function () { return ""; }, function (i) {
      return "Movimento " + (i + 1);
    });
  }

  function renderStagesResult(j, holdHtml) {
    // Com muitas etapas (cubos grandes chegam a 14) a linha vira uma parede de
    // texto: agrupa etapas de mesmo nome e resume o resto.
    var resumo = [];
    j.stages.forEach(function (s) {
      var nome = s.name.split(" — ")[0].split(" (")[0];
      var ult = resumo[resumo.length - 1];
      if (ult && ult.nome === nome) { ult.mov += s.moves.length; ult.n++; }
      else resumo.push({ nome: nome, mov: s.moves.length, n: 1 });
    });
    var partes = ["<b>" + j.length + " movimentos</b>"];
    if (resumo.length <= 5) {
      resumo.forEach(function (r) { partes.push(r.nome + " " + r.mov); });
    } else {
      partes.push(resumo.length + " etapas");
      var maior = resumo.slice().sort(function (a, b) { return b.mov - a.mov; })[0];
      partes.push("maior: " + maior.nome + " (" + maior.mov + ")");
    }
    partes.push((j.time_ms / 1000).toFixed(1) + " s");
    $("result").innerHTML = partes.join(" &middot; ") + (holdHtml || "");
    var nomes = [];
    j.stages.forEach(function (s) { s.moves.forEach(function (m) { nomes.push(m); }); });
    var stageOf = j.stage_of || [];
    definirLista(
      nomes,
      function (i) { return "stg-" + ((stageOf[i] || 0) % 4); },
      function (i) {
        var st = j.stages[stageOf[i]];
        return (st ? st.name + " — " : "") + "movimento " + (i + 1);
      }
    );
  }

  /** Rótulo da lista fechada, para o usuário saber o que há dentro. */
  function marcarLista(total) {
    $("lista-sum").textContent = "Ver todos os " + total + " movimentos";
    $("lista-wrap").open = total <= 40; // solução curta cabe aberta
  }

  function solveLabel() {
    return N === 2 ? "Resolver (ótimo)" : N >= 4 ? "Resolver por etapas" : "Resolver";
  }

  function setSize(n) {
    N = n;
    document.body.className = "size-" + n;
    buildNet();
    buildCube3d();
    state = solvedString().split("");
    shown = "";
    stateChanged("");
    $("btn-solve").textContent = solveLabel();
  }

  // Predefinicoes de busca; mudar o modo preenche os campos avancados.
  var MODES = {
    fast: { target: 20, max: 20, timeout: 300, min: 0 },
    balanced: { target: 20, max: 20, timeout: 4000, min: 60 },
    best: { target: 15, max: 20, timeout: 10000, min: 0 },
    optimal: { target: 0, max: 20, timeout: 60000, min: 0 },
  };
  var modeMin = MODES.balanced.min;

  function applyMode(name) {
    var m = MODES[name];
    if (!m) return;
    $("opt-target").value = m.target;
    $("opt-max").value = m.max;
    $("opt-timeout").value = m.timeout;
    modeMin = m.min;
  }

  function gripBody(body) {
    // pegada escolhida vale para todos os solvers do 3x3
    var b = $("cfop-base").value;
    var f = $("cfop-front").value;
    if (b) body.base = b;
    if (f) body.front = f;
    return body;
  }

  function solveBody() {
    var body = gripBody({
      facelets: state.join(""),
      max_len: Math.max(1, +$("opt-max").value || 20),
      target_len: Math.max(0, +$("opt-target").value || 0),
      timeout_ms: Math.max(50, +$("opt-timeout").value || 4000),
      min_ms: modeMin,
    });
    if ($("mode").value === "optimal") body.optimal = true;
    var th = +$("opt-threads").value;
    if (th > 0) body.threads = th;
    return body;
  }

  // ------------------------------------------------------------- modo otimo
  // roda como "job" no servidor: a pagina consulta o progresso e pode cancelar
  var optJob = null; // { id, timer }

  function stopOptimalJob(cancelServer) {
    if (!optJob) return;
    clearInterval(optJob.timer);
    if (cancelServer) fetch("/api/optimal/cancel/" + optJob.id, { method: "POST" });
    optJob = null;
    var btn = $("btn-solve");
    btn.disabled = false;
    btn.textContent = "Resolver";
  }

  function fmtNodes(n) {
    if (n >= 1e9) return (n / 1e9).toFixed(1) + " B";
    if (n >= 1e6) return (n / 1e6).toFixed(0) + " M";
    return n.toLocaleString("pt-BR");
  }

  function startOptimalJob() {
    var btn = $("btn-solve");
    resetSolution();
    refresh();
    say("");
    var body = gripBody({
      facelets: state.join(""),
      timeout_ms: Math.max(500, +$("opt-timeout").value || 60000),
    });
    var th = +$("opt-threads").value;
    if (th > 0) body.threads = th;
    btn.disabled = true;
    btn.textContent = "Iniciando...";

    api("/api/optimal/start", body)
      .then(function (j) {
        btn.disabled = false;
        btn.textContent = "Cancelar prova";
        var id = j.job;
        optJob = {
          id: id,
          timer: setInterval(function () {
            fetch("/api/optimal/status/" + id)
              .then(function (r) { return r.json(); })
              .then(function (s) {
                if (!optJob || optJob.id !== id) return;
                if (!s.done) {
                  say(
                    "Provando: não existe com menos de " + s.lower_bound +
                    " · melhor até agora " + s.best_len +
                    " · " + fmtNodes(s.nodes) + " nós · " +
                    Math.round(s.elapsed_ms / 1000) + "s", "");
                  return;
                }
                stopOptimalJob(false);
                if (s.error) { say(s.error, "err"); return; }
                solution = s.result;
                renderSolution(s.result);
                jump(0);
              })
              .catch(function () { stopOptimalJob(false); say("perdi contato com o servidor", "err"); });
          }, 700),
        };
      })
      .catch(function (e) {
        btn.disabled = false;
        btn.textContent = "Resolver";
        say(e.message, "err");
      });
  }

  function doSolve() {
    if (N !== 3) {
      doSolveOther();
      return;
    }
    var n = missingCount();
    if (n > 0) {
      say("Faltam " + n + " adesivo" + (n > 1 ? "s" : "") + " para poder resolver.", "err");
      return;
    }
    if ($("mode").value === "optimal") {
      startOptimalJob();
      return;
    }
    var btn = $("btn-solve");
    btn.disabled = true;
    btn.textContent = "Resolvendo...";
    var t0 = Date.now();
    var tick = setInterval(function () {
      btn.textContent = "Resolvendo... " + Math.round((Date.now() - t0) / 1000) + "s";
    }, 1000);
    resetSolution();
    refresh();
    say("");

    api("/api/solve", solveBody())
      .then(function (j) {
        solution = j;
        renderSolution(j);
        jump(0);
      })
      .catch(function (e) {
        resetSolution();
        refresh();
        say(e.message, "err");
      })
      .finally(function () {
        clearInterval(tick);
        btn.disabled = false;
        btn.textContent = "Resolver";
      });
  }

  // ------------------------------------------------------------ CFOP
  function doCfop() {
    if (optJob || N !== 3) return;
    var n = missingCount();
    if (n > 0) {
      say("Faltam " + n + " adesivo" + (n > 1 ? "s" : "") + " para poder resolver.", "err");
      return;
    }
    var btn = $("btn-cfop");
    btn.disabled = true;
    btn.textContent = "Resolvendo...";
    resetSolution();
    refresh();
    say("");
    api("/api/cfop", {
      facelets: state.join(""),
      base: $("cfop-base").value,
      front: $("cfop-front").value,
    })
      .then(function (j) {
        // adapta para o formato do player, com as etapas junto
        var names = [];
        j.stages.forEach(function (s) { s.moves.forEach(function (m) { names.push(m); }); });
        solution = {
          solution: names,
          states: j.states,
          length: j.length,
          stageOf: j.stage_of,
          stages: j.stages,
          hold: j.hold,
        };
        renderCfop(j);
        jump(0);
      })
      .catch(function (e) {
        resetSolution();
        refresh();
        say(e.message, "err");
      })
      .finally(function () {
        btn.disabled = false;
        btn.textContent = "Por etapas (CFOP)";
      });
  }

  function renderCfop(j) {
    var partes = ["<b>" + j.length + " movimentos</b>"];
    j.stages.forEach(function (s) {
      partes.push(s.name.split(" — ")[0].split(" (")[0] + " " + s.moves.length);
    });
    partes.push(j.time_ms + " ms");
    $("result").innerHTML = partes.join(" &middot; ") +
      "<br><b>" + j.hold + "</b> O cubo 3D já está nessa orientação.";

    var box = $("moves");
    box.innerHTML = "";
    var flat = 0;
    j.stages.forEach(function (s, si) {
      s.moves.forEach(function (m) {
        var b = document.createElement("button");
        b.className = "mv stg-" + (si % 4);
        b.textContent = m;
        b.title = s.name + " — movimento " + (flat + 1) + ": " + moveDesc(m);
        (function (i) {
          b.addEventListener("click", function () { stopPlay(); jump(i); });
        })(flat);
        box.appendChild(b);
        flat++;
      });
    });

    $("player").classList.remove("hidden");
    $("move-now").classList.remove("hidden");
    $("p-range").max = j.length;
    $("p-range").value = 0;
    say("Solução por etapas pronta. Siga etapa por etapa no player.", "ok");
  }

  function renderSolution(j) {
    if (j.length === 0) {
      $("result").innerHTML = "<b>Este cubo já está resolvido.</b>";
      return;
    }
    var parts = [
      "<b>" + j.length + " movimentos</b>",
      (j.time_ms >= 2000 ? (j.time_ms / 1000).toFixed(1) + " s" : j.time_ms + " ms"),
      j.nodes.toLocaleString("pt-BR") + " nós em " + j.threads + " threads",
    ];
    if (j.optimal === true) {
      parts.push("<b class=\"opt-ok\">ÓTIMO — provado que não existe menor</b>");
    } else if (typeof j.lower_bound === "number") {
      parts.push("provado que não existe com menos de <b>" + j.lower_bound + "</b> (prova incompleta no tempo dado)");
    } else {
      parts.push("fase 1: " + j.phase1 + " / fase 2: " + j.phase2);
      if (j.solutions > 1) parts.push(j.solutions + " soluções, ficou a melhor");
    }
    $("result").innerHTML = parts.join(" &middot; ") +
      (j.hold ? "<br><b>" + j.hold + "</b> O cubo 3D já está nessa orientação." : "");

    definirLista(
      j.solution,
      function (i) { return i >= j.phase1 ? "p2" : ""; },
      function (i) { return "Movimento " + (i + 1) + ": " + moveDesc(j.solution[i]); }
    );

    $("player").classList.remove("hidden");
    $("move-now").classList.remove("hidden");
    $("p-range").max = j.length;
    $("p-range").value = 0;
    say("Solução encontrada. Aperte ▶ tocar ou avance um movimento por vez.", "ok");
  }

  /**
   * step = quantos movimentos ja foram aplicados. O cubo mostra o estado ANTES
   * do proximo movimento, que fica destacado com a seta — e o que a pessoa com
   * o cubo na mao precisa fazer agora.
   */
  function setStep(i) {
    if (!solution || solution.length === 0) return;
    step = Math.max(0, Math.min(solution.length, i));
    draw(solution.states[step]);
    $("p-range").value = step;
    $("p-counter").textContent = step + " / " + solution.length;
    // acompanha o movimento atual na lista, sem precisar rolar à mão
    if ($("lista-wrap").open) {
      desenharLista();
      var base = listaJanela.de < 0 ? 0 : listaJanela.de;
      var idx = 0;
      Array.prototype.forEach.call($("moves").children, function (el) {
        if (!el.classList.contains("mv")) return; // avisos "… N antes"
        var k = base + idx;
        idx++;
        el.classList.toggle("done", k < step);
        el.classList.toggle("now", k === step);
      });
      var atual = $("moves").querySelector(".mv.now");
      if (atual) atual.scrollIntoView({ block: "nearest", inline: "nearest" });
    }

    var mn = $("move-now");
    var pct = solution.length ? (step / solution.length) * 100 : 0;
    $("mn-barra").style.width = pct.toFixed(1) + "%";
    if (step < solution.length) {
      var name = solution.solution[step];
      mn.classList.remove("done");
      $("mn-chip").textContent = name;
      $("mn-title").textContent = "Movimento " + (step + 1) + " de " + solution.length;
      $("mn-text").textContent = moveDesc(name);
      $("mn-etapa").textContent = etapaAtual(step);
      $("mn-prox").textContent = solution.solution[step + 1] || "fim";
    } else {
      mn.classList.add("done");
      $("mn-chip").textContent = "✓";
      $("mn-title").textContent = "Pronto!";
      $("mn-text").textContent = "Cubo resolvido em " + solution.length + " movimentos.";
      $("mn-etapa").textContent = "";
      $("mn-prox").textContent = "—";
      $("mn-barra").style.width = "100%";
    }
  }

  /** "Etapa 3 de 14 · Agrupar aresta cima-direita (12 de 34)". No 3x3 direto
   *  não há etapas nomeadas, mas o algoritmo tem duas fases bem definidas —
   *  usá-las dá a noção de onde se está. */
  function etapaAtual(i) {
    if (!solution.stageOf || !solution.stages) {
      if (typeof solution.phase1 === "number" && solution.phase1 > 0) {
        return i < solution.phase1
          ? "Fase 1 de 2 · orienta as peças (" + (i + 1) + " de " + solution.phase1 + ")"
          : "Fase 2 de 2 · resolve o resto (" + (i - solution.phase1 + 1) + " de " +
            (solution.length - solution.phase1) + ")";
      }
      return "Resolvendo — " + Math.round((i / solution.length) * 100) + "% dos movimentos";
    }
    var si = solution.stageOf[i];
    var st = solution.stages[si];
    var inicio = solution.stageOf.indexOf(si);
    return "Etapa " + (si + 1) + " de " + solution.stages.length + " · " + st.name +
      " (" + (i - inicio + 1) + " de " + st.moves.length + ")";
  }

  /** Pulo direto (slider, clique num movimento, inicio/fim): sem animacao. */
  function jump(i) {
    abortAnim();
    setStep(i);
  }

  /** Avanca um passo girando a camada na tela — em qualquer tamanho, inclusive
   *  os movimentos de camada grossa (Rw, 3Rw), que giram 2 ou 3 camadas. */
  function stepForward() {
    if (!solution || solution.length === 0) return;
    finishAnim(); // se ja tinha uma girando, aterrissa ela primeiro
    if (step >= solution.length) return;
    var target = step + 1;
    var name = solution.solution[step];
    if (!TURN[baseName(name)[0]]) {
      jump(target); // notação inesperada: aplica sem animar
      return;
    }
    animateMove(name, function () { setStep(target); });
  }

  function stopPlay() {
    if (timer) { clearInterval(timer); timer = null; }
    $("p-play").innerHTML = "▶ tocar";
  }

  function togglePlay() {
    if (timer) { stopPlay(); return; }
    if (!solution || solution.length === 0) return;
    if (step >= solution.length) jump(0);
    $("p-play").innerHTML = "⏸ pausar";
    stepForward();
    timer = setInterval(function () {
      if (!solution || step >= solution.length) { stopPlay(); return; }
      stepForward();
    }, passoMs());
  }

  // Teclas 1 a 6 pintam no guiado (bem mais rápido que mirar o mouse na cor):
  // a ordem é a mesma da paleta — 1 branca, 2 vermelha, 3 verde, 4 amarela,
  // 5 laranja, 6 azul. Backspace desfaz.
  document.addEventListener("keydown", function (e) {
    if (!guided) return;
    var alvo = e.target;
    if (alvo && (alvo.tagName === "INPUT" || alvo.tagName === "SELECT")) return;
    if (e.key === "Backspace") { e.preventDefault(); guidedUndo(); return; }
    var n = "123456".indexOf(e.key);
    if (n < 0) return;
    e.preventDefault();
    var sw = $("palette").children[n];
    if (!sw) return;
    if (sw.classList.contains("off")) {
      say("A cor " + FACE_LABEL[FACES[n]] + " não cabe nesse adesivo.", "err");
      sw.classList.add("recusada");
      setTimeout(function () { sw.classList.remove("recusada"); }, 300);
      return;
    }
    guidedPaint(FACES[n]);
  });

  // a lista longa só é desenhada quando aberta (montar 1183 fichas travava a página)
  $("lista-wrap").addEventListener("toggle", function () { desenharLista(); });

  // --------------------------------------------------------------- init
  buildPalette();
  buildNet();
  bindNetEvents();
  buildCube3d();
  bindCameraEvents();
  document.body.className = "size-3";
  refresh();

  $("btn-guided").addEventListener("click", enterGuided);
  $("btn-undo").addEventListener("click", guidedUndo);
  $("btn-exit-guided").addEventListener("click", function () {
    exitGuided("Modo guiado encerrado — a pintura ficou como está.");
  });
  $("btn-scramble").addEventListener("click", doScramble);
  $("btn-solved").addEventListener("click", function () {
    state = solvedString().split("");
    stateChanged("");
  });
  $("btn-clear").addEventListener("click", function () {
    state = ".".repeat(totalStickers()).split("");
    fillFixedCenters();
    stateChanged();
  });
  $("size").addEventListener("change", function (e) { setSize(+e.target.value); });
  $("btn-apply").addEventListener("click", doApply);
  $("btn-cfop").addEventListener("click", doCfop);
  $("seq").addEventListener("keydown", function (e) { if (e.key === "Enter") doApply(); });
  $("btn-solve").addEventListener("click", function () {
    if (optJob) {
      // o botao vira "Cancelar prova" durante um job do modo otimo
      say("Cancelando...", "");
      fetch("/api/optimal/cancel/" + optJob.id, { method: "POST" });
      return; // o poll entrega o resultado parcial em seguida
    }
    doSolve();
  });
  $("mode").addEventListener("change", function (e) { applyMode(e.target.value); });
  applyMode($("mode").value);

  // pegada: frentes iguais/opostas a base ficam bloqueadas, com aviso imediato
  var OPPOSITE = { U: "D", D: "U", R: "L", L: "R", F: "B", B: "F" };
  var GRIP_NAMES = { U: "branca", R: "vermelha", F: "verde", D: "amarela", L: "laranja", B: "azul" };
  function refreshGrip() {
    var b = $("cfop-base").value;
    var fsel = $("cfop-front");
    Array.prototype.forEach.call(fsel.options, function (op) {
      op.disabled = b !== "" && op.value !== "" && (op.value === b || op.value === OPPOSITE[b]);
    });
    if (fsel.selectedOptions[0] && fsel.selectedOptions[0].disabled) fsel.value = "";
    var f = fsel.value;
    if (b || f) {
      say(
        "Pegada: " + (b ? GRIP_NAMES[b] : "como pintei") + " embaixo" +
        (f ? ", " + GRIP_NAMES[f] + " na frente" : "") +
        " — vale para Resolver, ótimo e CFOP.", "");
    }
  }
  $("cfop-base").addEventListener("change", refreshGrip);
  $("cfop-front").addEventListener("change", refreshGrip);

  $("p-first").addEventListener("click", function () { stopPlay(); jump(0); });
  $("p-prev").addEventListener("click", function () { stopPlay(); jump(step - 1); });
  $("p-next").addEventListener("click", function () { stopPlay(); stepForward(); });
  $("p-last").addEventListener("click", function () {
    stopPlay();
    if (solution) jump(solution.length);
  });
  $("p-play").addEventListener("click", togglePlay);
  $("p-range").addEventListener("input", function (e) { stopPlay(); jump(+e.target.value); });

  document.addEventListener("keydown", function (e) {
    if (e.target.tagName === "INPUT" || e.target.tagName === "SELECT") return;
    if (e.key === "ArrowRight") { stopPlay(); stepForward(); }
    if (e.key === "ArrowLeft") { stopPlay(); jump(step - 1); }
    if (e.key === " ") { e.preventDefault(); togglePlay(); }
  });

  fetch("/api/health")
    .then(function (r) { if (r.ok) $("engine").textContent = "motor Rust · online"; })
    .catch(function () { $("engine").textContent = "motor Rust · offline"; });
})();
