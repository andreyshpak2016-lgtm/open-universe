/* Open Universe — рендер плана и живого прогресса. */
(function () {
  "use strict";

  var LABEL = {
    get done() { return t("js.status.done"); },
    get in_progress() { return t("js.status.in_progress"); },
    get todo() { return t("js.status.todo"); }
  };
  var GLYPH = { done: "✓", in_progress: "…", todo: "" };

  function pct(list) {
    if (!list.length) return 0;
    var done = list.filter(function (t) { return t.status === "done"; }).length;
    var wip = list.filter(function (t) { return t.status === "in_progress"; }).length;
    return Math.round(((done + wip * 0.5) / list.length) * 100);
  }

  function el(tag, cls, html) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (html != null) n.innerHTML = html;
    return n;
  }

  /** Поле с учётом языка: для «en» берём суффикс _en, иначе — базовое поле. */
  function LF(obj, field) {
    if (!obj) return "";
    if (currentLang() === "en" && obj[field + "_en"] != null) return obj[field + "_en"];
    return obj[field] != null ? obj[field] : "";
  }

  function buildPhase(phase, index) {
    var tasks = phase.tasks || [];
    var p = pct(tasks);
    var node = el("div", "panel phase" + (index === 0 ? " open" : ""));

    var head = el("div", "phase-head");
    head.innerHTML =
      '<span class="caret">▶</span>' +
      '<span class="num">' + t("js.phase") + " " + index + '</span>' +
      '<div><h3>' + LF(phase, "title") + '</h3></div>' +
      '<span class="goal">' + LF(phase, "goal") + '</span>' +
      '<span class="bar"><i style="width:' + p + '%"></i></span>' +
      '<span class="pct">' + p + '%</span>';
    node.appendChild(head);

    var ul = el("ul", "tasks");
    tasks.forEach(function (t) {
      var li = el("li", t.status);
      li.innerHTML =
        '<span class="check ' + t.status + '">' + (GLYPH[t.status] || "") + '</span>' +
        '<span class="t">' + LF(t, "title") + '</span>' +
        '<span class="badge ' + t.status + '">' + (LABEL[t.status] || t.status) + '</span>';
      ul.appendChild(li);
    });
    node.appendChild(ul);

    head.addEventListener("click", function () { node.classList.toggle("open"); });
    // Задачи видны только у раскрытой фазы
    ul.style.display = node.classList.contains("open") ? "" : "none";
    new MutationObserver(function () {
      ul.style.display = node.classList.contains("open") ? "" : "none";
    }).observe(node, { attributes: true, attributeFilter: ["class"] });

    return node;
  }

  function render() {
    var plan = window.PLAN;
    if (!plan) return;

    // Plan list
    var list = document.getElementById("plan-list");
    list.innerHTML = "";
    plan.phases.forEach(function (ph, i) { list.appendChild(buildPhase(ph, i)); });

    // Overall stats
    var all = plan.phases.reduce(function (acc, ph) { return acc.concat(ph.tasks || []); }, []);
    var done = all.filter(function (t) { return t.status === "done"; }).length;
    var wip = all.filter(function (t) { return t.status === "in_progress"; }).length;
    var todo = all.length - done - wip;
    var overall = pct(all);

    document.getElementById("ring-pct").textContent = overall + "%";
    document.getElementById("ring").style.setProperty("--p", overall);
    document.getElementById("stats").innerHTML =
      '<div><b style="color:var(--done)">' + done + '</b><span>' + t("js.done") + '</span></div>' +
      '<div><b style="color:var(--wip)">' + wip + '</b><span>' + t("js.wip") + '</span></div>' +
      '<div><b style="color:var(--todo)">' + todo + '</b><span>' + t("js.todo") + '</span></div>' +
      '<div><b>' + plan.phases.length + '</b><span>' + t("js.phases") + '</span></div>';

    // Timeline
    var tl = document.getElementById("timeline");
    tl.innerHTML = "";
    plan.phases.forEach(function (ph, i) {
      var tasks = ph.tasks || [];
      var p = pct(tasks);
      var state = tasks.length && tasks.every(function (t) { return t.status === "done"; })
        ? "done"
        : (tasks.some(function (t) { return t.status !== "todo"; }) ? "in_progress" : "");
      var item = el("div", "tl-item " + state);
      item.innerHTML = '<b>' + t("js.phase") + " " + i + ' — ' + LF(ph, "title") + '</b>' +
        '<span style="color:var(--muted);font-size:.88rem">' + LF(ph, "goal") +
        ' · <span style="color:var(--txt)">' + p + '%</span></span>';
      tl.appendChild(item);
    });

    document.getElementById("updated").textContent = plan.updated || "—";
  }

  /* ---------- Вкладки ---------- */
  function initTabs() {
    var tabs = Array.prototype.slice.call(document.querySelectorAll(".tab"));
    tabs.forEach(function (tab) {
      tab.addEventListener("click", function () {
        tabs.forEach(function (t) { t.classList.remove("active"); });
        tab.classList.add("active");
        document.querySelectorAll(".tab-panel").forEach(function (p) { p.classList.remove("active"); });
        var panel = document.getElementById(tab.getAttribute("data-tab"));
        if (panel) panel.classList.add("active");
        // Пересчёт интерактивных схем после показа вкладки
        window.dispatchEvent(new Event("resize"));
      });
    });
  }

  /* ---------- Интерактивная схема слоёв ---------- */
  // Тексты берутся из i18n.js по ключу — чтобы схема переводилась вместе с сайтом.
  var LAYERS = [
    { key: "client",   color: "#6a8bff" },
    { key: "network",  color: "#00e6d0" },
    { key: "server",   color: "#00e6d0" },
    { key: "services", color: "#a78bfa" },
    { key: "storage",  color: "#ffc24b" },
    { key: "infra",    color: "#8ea3d8" }
  ];

  function initLayers() {
    var host = document.getElementById("layers-list");
    if (!host) return;
    host.innerHTML = "";
    LAYERS.forEach(function (l, i) {
      var node = el("div", "layer" + (i === 0 ? " open" : ""));
      var chips = t("js.layers." + l.key + ".chips").split("\n")
        .map(function (c) { return "<span>" + c + "</span>"; }).join("");
      node.innerHTML =
        '<div class="layer-head">' +
          '<span class="swatch" style="background:linear-gradient(180deg,' + l.color + ',' + l.color + '55)"></span>' +
          '<div><h3>' + t("js.layers." + l.key + ".name") + '</h3><span class="tag">' + t("js.layers." + l.key + ".tag") + '</span></div>' +
          '<span class="caret2">▶</span>' +
        '</div>' +
        '<div class="layer-body"><div class="chips">' + chips + '</div><p>' + t("js.layers." + l.key + ".text") + '</p></div>';
      node.addEventListener("click", function () {
        var wasOpen = node.classList.contains("open");
        host.querySelectorAll(".layer").forEach(function (x) { x.classList.remove("open"); });
        if (!wasOpen) node.classList.add("open");
      });
      host.appendChild(node);
    });
  }

  /* ---------- Интерактивная ER-схема ---------- */
  var ER_W = 1500, ER_H = 1020, ER_BW = 250, ER_HDR = 40, ER_RH = 26;
  var ER_COLOR = { identity: "#6a8bff", world: "#00e6d0", content: "#a78bfa", economy: "#ffc24b" };

  var ER_TABLES = {
    accounts:        { x: 40,   y: 70,  group: "identity", desc: "js.er.accounts.desc", cols: [["id","PK"],["email"],["status"],["created_at"]] },
    characters:      { x: 370,  y: 70,  group: "identity", desc: "js.er.characters.desc", cols: [["id","PK"],["account_id","FK"],["org_id","FK"],["name"],["credits"]] },
    star_systems:    { x: 1080, y: 70,  group: "world",    desc: "js.er.star_systems.desc", cols: [["id","PK"],["name"],["seed"],["coords"]] },
    organizations:   { x: 40,   y: 700, group: "identity", desc: "js.er.organizations.desc", cols: [["id","PK"],["name"],["tag"],["owner_id","FK"]] },
    blueprints:      { x: 370,  y: 700, group: "content",  desc: "js.er.blueprints.desc", cols: [["id","PK"],["owner_id","FK"],["name"],["price"],["data_oid"]] },
    market_orders:   { x: 700,  y: 640, group: "economy",  desc: "js.er.market_orders.desc", cols: [["id","PK"],["system_id","FK"],["item_id","FK"],["seller_id","FK"],["side"],["price"],["qty"]] },
    territories:     { x: 1080, y: 700, group: "world",    desc: "js.er.territories.desc", cols: [["id","PK"],["system_id","FK"],["owner_id","FK"],["status"],["tax"]] },
    inventory_items: { x: 370,  y: 930, group: "content",  desc: "js.er.inventory_items.desc", cols: [["id","PK"],["owner_id","FK"],["blueprint_id","FK"],["qty"],["location"]] },
    transactions:    { x: 700,  y: 930, group: "economy",  desc: "js.er.transactions.desc", cols: [["id","PK"],["order_id","FK"],["buyer_id","FK"],["total"],["ts"]] },
    voxel_chunks:    { x: 1080, y: 930, group: "world",    desc: "js.er.voxel_chunks.desc", cols: [["id","PK"],["system_id","FK"],["coords"],["data_oid"],["version"]] }
  };

  var ER_LINKS = [
    ["accounts","characters","js.er.l.accounts-characters"],
    ["organizations","characters","js.er.l.organizations-characters"],
    ["accounts","organizations","js.er.l.accounts-organizations"],
    ["characters","blueprints","js.er.l.characters-blueprints"],
    ["blueprints","inventory_items","js.er.l.blueprints-inventory_items"],
    ["characters","market_orders","js.er.l.characters-market_orders"],
    ["blueprints","market_orders","js.er.l.blueprints-market_orders"],
    ["market_orders","transactions","js.er.l.market_orders-transactions"],
    ["star_systems","market_orders","js.er.l.star_systems-market_orders"],
    ["star_systems","territories","js.er.l.star_systems-territories"],
    ["characters","territories","js.er.l.characters-territories"],
    ["star_systems","voxel_chunks","js.er.l.star_systems-voxel_chunks"]
  ];

  function erEdges() {
    var out = {};
    Object.keys(ER_TABLES).forEach(function (k) { out[k] = []; });
    ER_LINKS.forEach(function (l) { out[l[0]].push(l[1]); out[l[1]].push(l[0]); });
    return out;
  }

  // Сброс выделения в ER-схеме — доступен из applyLang при смене языка.
  var ERD_clearAll = null;

  function erAnchor(a, b) {
    var ah = ER_HDR + ER_RH * a.cols.length, bh = ER_HDR + ER_RH * b.cols.length;
    var acx = a.x + ER_BW / 2, acy = a.y + ah / 2, bcx = b.x + ER_BW / 2, bcy = b.y + bh / 2;
    var dx = bcx - acx, dy = bcy - acy;
    var tx = dx !== 0 ? (ER_BW / 2) / Math.abs(dx) : 1e9;
    var ty = dy !== 0 ? (ah / 2) / Math.abs(dy) : 1e9;
    var t1 = Math.min(tx, ty);
    var ty2 = dy !== 0 ? (bh / 2) / Math.abs(dy) : 1e9;
    var tx2 = dx !== 0 ? (ER_BW / 2) / Math.abs(dx) : 1e9;
    var t2 = Math.min(tx2, ty2);
    return [acx + dx * t1, acy + dy * t1, bcx - dx * t2, bcy - dy * t2];
  }

  function initERD() {
    var graph = document.getElementById("erd-graph");
    if (!graph) return;
    var edges = erEdges();

    var sizer = el("div", "erd-sizer");
    var stage = el("div", "erd-stage");
    var svgNS = "http://www.w3.org/2000/svg";
    var svg = document.createElementNS(svgNS, "svg");
    svg.setAttribute("class", "erd-edges");
    svg.setAttribute("viewBox", "0 0 " + ER_W + " " + ER_H);

    var linkEls = [];
    ER_LINKS.forEach(function (l) {
      var p = erAnchor(ER_TABLES[l[0]], ER_TABLES[l[1]]);
      var path = document.createElementNS(svgNS, "path");
      path.setAttribute("d", "M " + p[0].toFixed(1) + " " + p[1].toFixed(1) + " L " + p[2].toFixed(1) + " " + p[3].toFixed(1));
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", "#5b6ea8");
      path.setAttribute("stroke-opacity", "0.5");
      path.setAttribute("stroke-width", "1.6");
      svg.appendChild(path);
      var mid = document.createElementNS(svgNS, "text");
      mid.setAttribute("class", "lbl");
      mid.setAttribute("x", ((p[0] + p[2]) / 2).toFixed(1));
      mid.setAttribute("y", ((p[1] + p[3]) / 2 - 5).toFixed(1));
      mid.setAttribute("text-anchor", "middle");
      mid.setAttribute("font-size", "11");
      mid.setAttribute("fill", "#93a0c4");
      mid.setAttribute("opacity", "0");
      mid.textContent = t(l[2]);
      svg.appendChild(mid);
      linkEls.push({ a: l[0], b: l[1], path: path, label: mid });
    });

    stage.appendChild(svg);

    var nodeEls = {};
    Object.keys(ER_TABLES).forEach(function (key) {
      var t = ER_TABLES[key];
      var c = ER_COLOR[t.group];
      var node = el("div", "tnode");
      node.style.left = t.x + "px";
      node.style.top = t.y + "px";
      node.style.width = ER_BW + "px";
      node.style.setProperty("--rd-accent", c);
      var rows = t.cols.map(function (col) {
        return '<div class="tnode-row"><span>' + col[0] + '</span>' +
          (col[1] ? '<span class="k ' + col[1] + '">' + col[1] + '</span>' : '') + '</div>';
      }).join("");
      node.innerHTML =
        '<div class="tnode-head" style="background:linear-gradient(90deg,' + c + '2e,' + c + '0a);border-bottom:1px solid ' + c + '40">' + key + '</div>' +
        '<div class="tnode-body">' + rows + '</div>';
      node.addEventListener("click", function (e) { e.stopPropagation(); select(key); });
      stage.appendChild(node);
      nodeEls[key] = node;
    });

    sizer.appendChild(stage);
    graph.appendChild(sizer);

    function fit() {
      var avail = graph.clientWidth;
      var s = Math.min(1, avail / ER_W);
      stage.style.transform = "scale(" + s + ")";
      sizer.style.height = (ER_H * s) + "px";
    }
    window.addEventListener("resize", fit);

    function clear() {
      Object.keys(nodeEls).forEach(function (k) { nodeEls[k].classList.remove("sel", "rel", "dim"); });
      linkEls.forEach(function (le) {
        le.path.setAttribute("stroke", "#5b6ea8");
        le.path.setAttribute("stroke-opacity", "0.5");
        le.path.setAttribute("stroke-width", "1.6");
        le.label.setAttribute("opacity", "0");
      });
      document.getElementById("erd-title").textContent = t("js.erd.title");
      document.getElementById("erd-desc").textContent = t("js.erd.desc");
      document.getElementById("erd-cols").innerHTML = '<li class="muted">' + t("js.erd.colsEmpty") + '</li>';
      document.getElementById("erd-links").innerHTML = '<span class="muted">—</span>';
    }

    function select(key) {
      var tbl = ER_TABLES[key];
      var c = ER_COLOR[tbl.group];
      var related = edges[key] || [];
      Object.keys(nodeEls).forEach(function (k) {
        var n = nodeEls[k];
        n.classList.remove("sel", "rel", "dim");
        if (k === key) n.classList.add("sel");
        else if (related.indexOf(k) >= 0) n.classList.add("rel");
        else n.classList.add("dim");
      });
      linkEls.forEach(function (le) {
        var on = le.a === key || le.b === key;
        le.path.setAttribute("stroke", on ? c : "#5b6ea8");
        le.path.setAttribute("stroke-opacity", on ? "1" : "0.12");
        le.path.setAttribute("stroke-width", on ? "2.6" : "1.4");
        le.label.setAttribute("opacity", on ? "1" : "0");
      });
      document.getElementById("erd-title").textContent = key;
      document.getElementById("erd-desc").textContent = t(tbl.desc);
      document.getElementById("erd-cols").innerHTML = tbl.cols.map(function (col) {
        return '<li><span>' + col[0] + '</span>' + (col[1] ? '<span class="k ' + col[1] + '">' + col[1] + '</span>' : '') + '</li>';
      }).join("");
      var links = related.map(function (k) { return '<span data-go="' + k + '">' + k + '</span>'; }).join("");
      document.getElementById("erd-links").innerHTML = links || '<span class="muted">' + t("js.erd.noLinks") + '</span>';
    }

    document.getElementById("erd-links").addEventListener("click", function (e) {
      var g = e.target.getAttribute("data-go");
      if (g) select(g);
    });
    document.getElementById("erd-reset").addEventListener("click", clear);
    graph.addEventListener("click", function (e) { if (e.target === graph || e.target === sizer) clear(); });

    clear();
    ERD_clearAll = clear;
    requestAnimationFrame(fit);
    window.addEventListener("resize", function () { requestAnimationFrame(fit); });
  }

  /* ---------- Скачивание: данные последнего релиза ---------- */
  function loadLatestRelease() {
    var repo = "andreyshpak2016-lgtm/open-universe";
    function apply(rel) {
      if (!rel) return;
      var verEl = document.getElementById("dl-version");
      if (verEl && rel.tag) verEl.textContent = rel.tag;
      // Размер основной кнопки — лаунчер (маленький); дату берём у релиза.
      var sizeEl = document.getElementById("dl-size");
      if (sizeEl && rel.launcherSize) {
        sizeEl.textContent = "~" + Math.max(1, Math.round(rel.launcherSize / 1024)) + " " + t("js.unit.kb");
      }
      var dateEl = document.getElementById("dl-date");
      if (dateEl && rel.date) dateEl.textContent = new Date(rel.date).toLocaleDateString(t("js.locale"));
      // Кнопка лаунчера: прямая ссылка на файл в последнем релизе.
      var launcherEl = document.getElementById("dl-launcher");
      if (launcherEl && rel.launcher) launcherEl.href = rel.launcher;
      // Кнопка «Игра (установщик)» — установщик с графикой.
      var linkEl = document.getElementById("dl-latest");
      if (linkEl && (rel.gameSetup || rel.setup || rel.zip || rel.client)) {
        linkEl.href = rel.gameSetup || rel.setup || rel.zip || rel.client;
      }
    }
    // 1) releases.json рядом с сайтом (без GitHub API).
    fetch("releases.json", { cache: "no-store" })
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (doc) {
        if (doc && doc.releases && doc.releases.length) {
          var latest = doc.releases.filter(function (x) { return x.latest; })[0] || doc.releases[0];
          apply(latest);
          return;
        }
        throw new Error("empty");
      })
      .catch(function () {
        // 2) Запасной путь — GitHub API.
        fetch("https://api.github.com/repos/" + repo + "/releases/latest",
          { headers: { Accept: "application/vnd.github+json" }, cache: "no-store" })
          .then(function (r) { return r.ok ? r.json() : null; })
          .then(function (rel) {
            if (!rel || !rel.tag_name) return;
            var a = (rel.assets || [])[0];
            apply({
              tag: rel.tag_name,
              date: rel.published_at,
              size: a ? a.size : 0,
              zip: a ? a.browser_download_url : null
            });
          })
          .catch(function () {});
      });
  }

  document.getElementById("year").textContent = new Date().getFullYear();

  /* ---------- Регистрация пилота (открывает скачивание) ---------- */
  // Аккаунт хранится в браузере (localStorage) — без серверов и сбора данных.
  var REG_KEY = "ou.pilot";

  function getPilot() {
    try { return JSON.parse(localStorage.getItem(REG_KEY) || "null"); }
    catch (e) { return null; }
  }
  function setPilot(p) {
    try { localStorage.setItem(REG_KEY, JSON.stringify(p)); } catch (e) {}
  }
  function clearPilot() {
    try { localStorage.removeItem(REG_KEY); } catch (e) {}
  }

  function applyRegistration() {
    var pilot = getPilot();
    var registered = !!pilot;
    var form = document.getElementById("register-form");
    var unlocked = document.getElementById("dl-unlocked");
    var welcome = document.getElementById("dl-welcome");
    if (form) form.hidden = registered;
    if (unlocked) unlocked.hidden = !registered;
    if (welcome && pilot) {
      welcome.textContent = t("js.welcome") + " " + pilot.name +
        (pilot.email ? " · " + pilot.email : "") + ". " + t("js.unlocked");
    }
    // Список версий: кнопки скачивания доступны только после регистрации.
    renderVersions(LAST_VERSIONS);
    // Кнопка в правом верхнем углу: пока регистрации нет — ведёт к форме
    // (форма находится в разделе «Скачать», поэтому якорь — #download).
    var nav = document.querySelector(".nav-cta");
    if (nav) {
      nav.textContent = registered ? t("nav.download") : t("nav.register");
      nav.setAttribute("href", "#download");
    }
  }

  /** Показать форму регистрации и прокрутить к ней. */
  function requireRegistration(e) {
    if (e) e.preventDefault();
    var f = document.getElementById("register-form");
    if (f) {
      f.hidden = false;
      f.scrollIntoView({ behavior: "smooth", block: "center" });
      var n = document.getElementById("reg-name");
      if (n) n.focus();
    }
  }

  function initRegister() {
    var form = document.getElementById("register-form");
    if (form) {
      form.addEventListener("submit", function (e) {
        e.preventDefault();
        var nameEl = document.getElementById("reg-name");
        var emailEl = document.getElementById("reg-email");
        var rulesEl = document.getElementById("reg-rules");
        var errEl = document.getElementById("reg-error");
        var name = (nameEl.value || "").trim();
        function fail(msg) { errEl.textContent = msg; errEl.hidden = false; }
        if (name.length < 2) { fail(t("js.nameShort")); return; }
        if (!rulesEl.checked) { fail(t("js.needRules")); return; }
        errEl.hidden = true;
        setPilot({ name: name, email: (emailEl.value || "").trim(),
                   at: new Date().toISOString() });
        try { localStorage.setItem(REG_KEY, JSON.stringify(getPilot())); } catch (e2) {}
        applyRegistration();
        var dl = document.getElementById("download");
        if (dl) dl.scrollIntoView({ behavior: "smooth", block: "start" });
      });
    }
    var logout = document.getElementById("dl-logout");
    if (logout) {
      logout.addEventListener("click", function () {
        clearPilot();
        applyRegistration();
      });
    }
    // Клик по «#register» из навигации — показать форму (она и так на месте).
    // Кнопка в правом углу — это ссылка `#register`; элемента с таким id нет,
    // поэтому обычный переход ничего не делает. Перехватываем клик сами.
    document.querySelectorAll('a[href="#register"], .nav-cta').forEach(function (a) {
      a.addEventListener("click", function (e) {
        if (getPilot()) return; // уже зарегистрирован — пусть идёт к скачиванию
        requireRegistration(e);
      });
    });
    window.addEventListener("hashchange", function () {
      if (location.hash === "#register" && !getPilot()) requireRegistration();
    });
    if (location.hash === "#register" && !getPilot()) requireRegistration();
    applyRegistration();
  }

  /* ---------- Язык сайта (EN по умолчанию, RU переключателем) ---------- */
  var LANG_KEY = "ou.lang";

  function currentLang() {
    try {
      var v = localStorage.getItem(LANG_KEY);
      if (v === "ru" || v === "en") return v;
    } catch (e) {}
    return "en"; // английский — язык по умолчанию
  }

  function t(key) {
    var pack = (window.OU_I18N || {})[currentLang()] || {};
    if (pack[key] != null) return pack[key];
    var en = (window.OU_I18N || {}).en || {};
    return en[key] != null ? en[key] : key;
  }

  function applyLang() {
    var lang = currentLang();
    document.documentElement.lang = lang;

    // Тексты из разметки.
    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      el.innerHTML = t(el.getAttribute("data-i18n"));
    });
    document.querySelectorAll("[data-i18n-ph]").forEach(function (el) {
      el.setAttribute("placeholder", t(el.getAttribute("data-i18n-ph")));
    });

    // Переключатель — активная кнопка.
    document.querySelectorAll("#lang-switch button").forEach(function (b) {
      b.classList.toggle("active", b.getAttribute("data-lang") === lang);
    });

    // Динамические части.
    render();
    initLayers();
    if (ERD_clearAll) ERD_clearAll();
    renderVersions(LAST_VERSIONS);
    applyRegistration();
  }

  function setLang(lang) {
    try { localStorage.setItem(LANG_KEY, lang); } catch (e) {}
    applyLang();
  }

  function initLang() {
    var sw = document.getElementById("lang-switch");
    if (sw) {
      sw.querySelectorAll("button").forEach(function (b) {
        b.addEventListener("click", function () { setLang(b.getAttribute("data-lang")); });
      });
    }
  }

  function boot() { initLang(); applyLang(); initTabs(); initLayers(); initERD(); loadLatestRelease(); loadVersions(); initRegister(); }

  /* ---------- Все версии игры (из releases.json, без GitHub API) ---------- */
  // Статический файл рядом с сайтом: не зависит от лимита GitHub API (60/час)
  // и работает на любом хостинге, включая GitHub Pages.
  var FALLBACK_VERSIONS = [
    { tag: "v0.21.0", date: "2026-11-12T09:00:00Z", latest: true, size: 0,
      client: "https://github.com/andreyshpak2016-lgtm/open-universe/releases/download/v0.21.0/ou-netclient.exe",
      zip: "https://github.com/andreyshpak2016-lgtm/open-universe/releases/download/v0.21.0/open-universe-0.21.0-windows-x64.zip",
      page: "https://github.com/andreyshpak2016-lgtm/open-universe/releases/tag/v0.21.0" }
  ];

  var LAST_VERSIONS = null;

  function renderVersions(rels) {
    // null/undefined — список ещё не загружен, не трогаем заглушку «Загрузка…».
    if (rels === null || rels === undefined) return;
    LAST_VERSIONS = rels;
    var box = document.getElementById("versions");
    if (!box) return;
    if (!rels.length) { 
      var st = document.getElementById("versions-status");
      if (st) st.textContent = t("js.versionsUnavailable");
      return;
    }
    var registered = !!getPilot();
    // По умолчанию показываем только самые свежие версии; остальные — по кнопке.
    var SHOW = 3;
    var html = "";
    rels.forEach(function (rel, idx) {
      var tag = rel.tag || rel.name || "—";
      var mbVal = (rel.size / 1048576).toFixed(1);
      if (t("js.locale") === "ru-RU") mbVal = mbVal.replace(".", ",");
      var size = rel.size ? mbVal + " " + t("js.unit.mb") : "";
      var date = rel.date ? new Date(rel.date).toLocaleDateString(t("js.locale")) : "";
      var links = "";
      if (registered) {
        if (rel.gameSetup) {
          // Только игра: одну версию ставим одним установщиком.
          links += '<a class="version-btn primary" href="' + rel.gameSetup + '" target="_blank" rel="noopener">' + t("js.gameInstaller") + '</a>';
        } else {
          links += '<span class="version-na">' + t("js.noInstaller") + '</span>';
        }
      } else {
        // До регистрации кнопки ведут в форму, а не к файлу.
        links = '<button type="button" class="version-btn primary locked js-need-reg">' + t("js.needReg") + '</button>';
      }
      // Старые версии прячем под «показать ещё».
      var cls = idx >= SHOW ? "version version-old" : "version";
      var hidden = idx >= SHOW ? " hidden" : "";
      html += '<div class="' + cls + '"' + hidden + '>' +
        '<span class="version-tag">' + tag + (rel.latest ? ' <span class="version-latest">' + t("js.lastVer") + '</span>' : '') + '</span>' +
        '<span class="version-meta">' + [date, size].filter(Boolean).join(" · ") + '</span>' +
        '<span class="version-actions">' + links + '</span></div>';
    });
    // Кнопка раскрытия — только если старых версий больше нуля.
    if (rels.length > SHOW) {
      html += '<button type="button" class="version-toggle" id="versions-toggle">' +
        t("js.showOlder") + ' (' + (rels.length - SHOW) + ') ▾</button>';
    }
    box.innerHTML = html;
    // Кнопки-замки открывают регистрацию.
    box.querySelectorAll(".js-need-reg").forEach(function (b) {
      b.addEventListener("click", function (e) { requireRegistration(e); });
    });
    // Раскрытие/сворачивание старых версий.
    var toggle = document.getElementById("versions-toggle");
    if (toggle) {
      toggle.addEventListener("click", function () {
        var open = toggle.getAttribute("data-open") === "1";
        box.querySelectorAll(".version-old").forEach(function (v) {
          v.hidden = open;
        });
        toggle.setAttribute("data-open", open ? "0" : "1");
        toggle.textContent = open
          ? (t("js.showOlder") + ' (' + (rels.length - SHOW) + ') ▾')
          : (t("js.hideOlder") + ' ▴');
      });
    }
  }

  function loadVersions() {
    // 1) Статический releases.json рядом с сайтом (основной путь).
    // 2) Если его нет — пробуем GitHub API (на случай свежего деплоя).
    // 3) Если и он недоступен — показываем встроенный список последней версии.
    fetch("releases.json", { cache: "no-store" })
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (doc) {
        if (doc && doc.releases && doc.releases.length) { renderVersions(doc.releases); return; }
        throw new Error("empty releases.json");
      })
      .catch(function () {
        fetch("https://api.github.com/repos/andreyshpak2016-lgtm/open-universe/releases?per_page=30",
          { headers: { Accept: "application/vnd.github+json" }, cache: "no-store" })
          .then(function (r) { return r.ok ? r.json() : null; })
          .then(function (rels) {
            if (!rels || !rels.length) { renderVersions(FALLBACK_VERSIONS); return; }
            renderVersions(rels.map(function (rel) {
              var assets = rel.assets || [];
              var client = assets.filter(function (a) { return /ou-netclient.*\.exe$/i.test(a.name); })[0]
                        || assets.filter(function (a) { return /\.exe$/i.test(a.name); })[0];
              var zip = assets.filter(function (a) { return /\.zip$/i.test(a.name); })[0];
              var primary = client || zip;
              return {
                tag: rel.tag_name, date: rel.published_at,
                latest: rel.tag_name === rels[0].tag_name,
                size: primary ? primary.size : 0,
                client: client ? client.browser_download_url : null,
                zip: zip ? zip.browser_download_url : null,
                page: "https://github.com/andreyshpak2016-lgtm/open-universe/releases/tag/" + rel.tag_name
              };
            }));
          })
          .catch(function () { renderVersions(FALLBACK_VERSIONS); });
      });
  }
  if (document.readyState !== "loading") boot();
  else document.addEventListener("DOMContentLoaded", boot);
})();
