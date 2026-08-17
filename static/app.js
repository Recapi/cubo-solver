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
      painting = true;
      paint(+t.dataset.i, selected);
    });
    net.addEventListener("pointermove", function (e) {
      if (!painting) return;
      var el = document.elementFromPoint(e.clientX, e.clientY);
      if (el && el.classList.contains("st")) paint(+el.dataset.i, selected);
    });
    window.addEventListener("pointerup", function () { painting = false; });
    net.addEventListener("contextmenu", function (e) {
      var t = e.target.closest(".st");
      if (!t) return;
      e.preventDefault();
      paint(+t.dataset.i, ".");
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

    // camera arrastavel (e so isso — a camera nunca se mexe sozinha)
    var rx = -24, ry = -34, drag = null;
    var scene = $("scene");
    scene.addEventListener("pointerdown", function (e) {
      drag = { x: e.clientX, y: e.clientY, rx: rx, ry: ry };
      scene.classList.add("dragging");
      scene.setPointerCapture(e.pointerId);
    });
    scene.addEventListener("pointermove", function (e) {
      if (!drag) return;
      ry = drag.ry + (e.clientX - drag.x) * 0.5;
      rx = Math.max(-89, Math.min(89, drag.rx - (e.clientY - drag.y) * 0.5));
      c.style.transform = "rotateX(" + rx + "deg) rotateY(" + ry + "deg)";
    });
    var end = function () { drag = null; scene.classList.remove("dragging"); };
    scene.addEventListener("pointerup", end);
    scene.addEventListener("pointercancel", end);
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
    solution = null;
    step = 0;
    $("moves").innerHTML = "";
    $("result").innerHTML = "";
    $("player").classList.add("hidden");
    $("move-now").classList.add("hidden");
  }

  /** Chamado depois de qualquer alteracao em `state`. */
  function stateChanged(msg, kind) {
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
    var th = +$("opt-threads").value;
    if (th > 0) body.threads = th;
    return body;
  }

  function doSolve() {
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
        btn.disabled = false;
        btn.textContent = "Resolver";
      });
  }

  function renderSolution(j) {
    if (j.length === 0) {
      $("result").innerHTML = "<b>Este cubo já está resolvido.</b>";
      return;
    }
    $("result").innerHTML =
      "<b>" + j.length + " movimentos</b> &middot; " +
      j.time_ms + " ms &middot; " +
      j.nodes.toLocaleString("pt-BR") + " nós em " + j.threads + " threads &middot; " +
      "fase 1: " + j.phase1 + " / fase 2: " + j.phase2 +
      (j.solutions > 1 ? " &middot; " + j.solutions + " soluções, ficou a melhor" : "");

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
  $("btn-solve").addEventListener("click", doSolve);
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
