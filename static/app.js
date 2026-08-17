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
      b.title = "cor da face " + FACE_LABEL[f];
      b.innerHTML = "<span>" + FACE_LABEL[f] + "</span>";
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
    FACES.forEach(function (f, fi) {
      var pos = NET_POS[f];
      for (var k = 0; k < 9; k++) {
        var d = document.createElement("div");
        d.className = "st";
        d.style.gridRow = pos[0] + Math.floor(k / 3) + 1;
        d.style.gridColumn = pos[1] + (k % 3) + 1;
        d.dataset.i = fi * 9 + k;
        if (k === 4) d.classList.add("center");
        netCells[fi * 9 + k] = d;
        net.appendChild(d);
      }
    });

    net.addEventListener("pointerdown", function (e) {
      var t = e.target.closest(".st");
      if (!t) return;
      e.preventDefault();
      var i = +t.dataset.i;
      if (guided) {
        // no guiado, clicar num adesivo vazio escolhe ele como alvo
        if (i % 9 !== 4 && FACES.indexOf(state[i]) < 0) setTarget(i);
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

  function buildCube3d() {
    var c = $("cube3d");

    // 26 cubinhos, cada um com 6 planos (os internos ficam como "plastico")
    var byPos = {};
    for (var x = 0; x < 3; x++) {
      for (var y = 0; y < 3; y++) {
        for (var z = 0; z < 3; z++) {
          if (x === 1 && y === 1 && z === 1) continue;
          var el = document.createElement("div");
          el.className = "cubie";
          var t = "translate3d(" + (x - 1) * 44 + "px," + (y - 1) * 44 + "px," + (z - 1) * 44 + "px)";
          el.dataset.t = t;
          el.style.transform = t;
          var planes = {};
          FACES.forEach(function (p) {
            var s = document.createElement("div");
            s.className = "stk sp-" + p;
            s.dataset.base = "stk sp-" + p;
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
    // (mesma convencao do servidor: linha 0 = de cima, coluna 0 = da esquerda,
    // olhando de frente para a face)
    FACES.forEach(function (f, fi) {
      for (var k = 0; k < 9; k++) {
        var r = Math.floor(k / 3), col = k % 3, x, y, z;
        if (f === "U") { x = col; y = 0; z = r; }
        else if (f === "D") { x = col; y = 2; z = 2 - r; }
        else if (f === "F") { x = col; y = r; z = 2; }
        else if (f === "B") { x = 2 - col; y = r; z = 0; }
        else if (f === "R") { x = 2; y = r; z = 2 - col; }
        else { x = 0; y = r; z = col; } // L
        cubeCells[fi * 9 + k] = byPos[x + "," + y + "," + z][f];
      }
    });

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
  function layerOf(face) {
    return cubies.filter(function (cb) {
      if (face === "U") return cb.y === 0;
      if (face === "D") return cb.y === 2;
      if (face === "R") return cb.x === 2;
      if (face === "L") return cb.x === 0;
      if (face === "F") return cb.z === 2;
      return cb.z === 0; // B
    });
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
  function animateMove(name, done) {
    var t = TURN[name[0]];
    var deg = t[1] * (name.length > 1 && name[1] === "'" ? -1 : name.length > 1 && name[1] === "2" ? 2 : 1);
    var dur = name.length > 1 && name[1] === "2" ? 520 : 340;
    var layer = layerOf(name[0]);
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

  /** Desenha `str` (54 letras) na planificacao e no cubo 3d. */
  function draw(str) {
    var prev = shown;
    shown = str;
    for (var i = 0; i < 54; i++) {
      var k = cls(str[i]);
      var cell = netCells[i];
      cell.className = "st" + (i % 9 === 4 ? " center " : " ") + k;
      if (prev[i] !== str[i]) {
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
    FACE_FILL_ORDER.forEach(function (f) {
      var fi = FACES.indexOf(f);
      for (var k = 0; k < 9; k++) if (k !== 4) out.push(fi * 9 + k);
    });
    return out;
  }

  function clearTargetMark() {
    for (var i = 0; i < 54; i++) {
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
      state = ".".repeat(54).split("");
      for (var f = 0; f < 6; f++) state[f * 9 + 4] = FACES[f];
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
    if (orientTo) orientTo(FACES[Math.floor(pos / 9)]);
    var faltam = state.filter(function (c) { return FACES.indexOf(c) < 0; }).length;
    say("Pinte o adesivo destacado — cores impossíveis ficam bloqueadas (faltam " + faltam + ").");
    setPaletteAllowed([]); // trava enquanto consulta o servidor
    api("/api/allowed", { facelets: partialString(), pos: pos })
      .then(function (j) {
        if (!guided || guided.target !== pos) return;
        if (j.colors.length === 0) {
          // pintura previa (feita no modo livre) ja era impossivel
          say("O que já estava pintado é impossível — recomeçando só com os centros.", "err");
          state = ".".repeat(54).split("");
          for (var f = 0; f < 6; f++) state[f * 9 + 4] = FACES[f];
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
    api("/api/scramble", { length: 25 })
      .then(function (j) {
        state = j.facelets.split("");
        stateChanged("Embaralhado com: " + j.notation, "");
      })
      .catch(function (e) { say(e.message, "err"); });
  }

  function doApply() {
    var seq = $("seq").value.trim();
    if (!seq) return;
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

  function solveBody() {
    var body = {
      facelets: state.join(""),
      max_len: Math.max(1, +$("opt-max").value || 20),
      target_len: Math.max(0, +$("opt-target").value || 0),
      timeout_ms: Math.max(50, +$("opt-timeout").value || 4000),
      min_ms: modeMin,
    };
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
    var body = {
      facelets: state.join(""),
      timeout_ms: Math.max(500, +$("opt-timeout").value || 60000),
    };
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
    $("result").innerHTML = parts.join(" &middot; ");

    var box = $("moves");
    box.innerHTML = "";
    j.solution.forEach(function (m, i) {
      var b = document.createElement("button");
      b.className = "mv" + (i >= j.phase1 ? " p2" : "");
      b.textContent = m;
      b.title = "Movimento " + (i + 1) + ": " + moveDesc(m);
      b.addEventListener("click", function () { stopPlay(); jump(i); });
      box.appendChild(b);
    });

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
    Array.prototype.forEach.call($("moves").children, function (el, k) {
      el.classList.toggle("done", k < step);
      el.classList.toggle("now", k === step);
    });

    var mn = $("move-now");
    if (step < solution.length) {
      var name = solution.solution[step];
      mn.classList.remove("done");
      $("mn-chip").textContent = name;
      $("mn-title").textContent = "Movimento " + (step + 1) + " de " + solution.length;
      $("mn-text").textContent = moveDesc(name);
    } else {
      mn.classList.add("done");
      $("mn-chip").textContent = "✓";
      $("mn-title").textContent = "Pronto!";
      $("mn-text").textContent = "Cubo resolvido em " + solution.length + " movimentos.";
    }
  }

  /** Pulo direto (slider, clique num movimento, inicio/fim): sem animacao. */
  function jump(i) {
    abortAnim();
    setStep(i);
  }

  /** Avanca um passo girando a camada na tela. */
  function stepForward() {
    if (!solution || solution.length === 0) return;
    finishAnim(); // se ja tinha uma girando, aterrissa ela primeiro
    if (step >= solution.length) return;
    var target = step + 1;
    animateMove(solution.solution[step], function () { setStep(target); });
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
    }, 1000);
  }

  // --------------------------------------------------------------- init
  buildPalette();
  buildNet();
  buildCube3d();
  refresh();

  $("btn-guided").addEventListener("click", enterGuided);
  $("btn-undo").addEventListener("click", guidedUndo);
  $("btn-exit-guided").addEventListener("click", function () {
    exitGuided("Modo guiado encerrado — a pintura ficou como está.");
  });
  $("btn-scramble").addEventListener("click", doScramble);
  $("btn-solved").addEventListener("click", function () {
    state = SOLVED.split("");
    stateChanged("");
  });
  $("btn-clear").addEventListener("click", function () {
    state = ".".repeat(54).split("");
    for (var f = 0; f < 6; f++) state[f * 9 + 4] = FACES[f];
    stateChanged();
  });
  $("btn-apply").addEventListener("click", doApply);
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
