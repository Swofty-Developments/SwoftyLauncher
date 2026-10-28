(function () {
  "use strict";
  var token = new URLSearchParams(location.search).get("t") || "";
  var $ = function (id) { return document.getElementById(id); };
  var RC_LABEL = { none: "Green", sc: "Safety car", sc_ending: "Safety car ending", vsc: "Virtual safety car", vsc_ending: "VSC ending", red_flag: "Red flag" };
  var RC_ACTIONS = { sc: "Deploy the safety car?", vsc: "Deploy the virtual safety car?", end: "End the safety car period?", red_flag: "Throw a red flag?" };
  var role = "unknown";
  var gridKey = "";
  var autoBusy = false;
  var failures = 0;

  function api(path, body) {
    var opts = { headers: { "X-Steward-Token": token } };
    if (body !== undefined) {
      opts.method = "POST";
      opts.headers["Content-Type"] = "application/json";
      opts.body = JSON.stringify(body);
    }
    return fetch(path + (path.indexOf("?") < 0 ? "?t=" + encodeURIComponent(token) : ""), opts).then(function (r) { return r.json(); });
  }

  function toast(msg, bad) {
    var t = $("toast");
    t.textContent = msg;
    t.className = "toast" + (bad ? " bad" : "");
    t.hidden = false;
    clearTimeout(toast.h);
    toast.h = setTimeout(function () { t.hidden = true; }, 3000);
  }

  function send(path, body, okMsg) {
    return api(path, body).then(function (r) {
      if (r && r.ok === false) toast(r.error || "Failed", true);
      else toast(okMsg || "Queued");
    }, function () { toast("No contact with the game", true); });
  }

  function confirmBox(text, fn) {
    $("modal-text").textContent = text;
    $("modal").hidden = false;
    $("modal-no").onclick = function () { $("modal").hidden = true; };
    $("modal-yes").onclick = function () { $("modal").hidden = true; fn(); };
  }

  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  function penaltyBtn(d, label, body, ask) {
    var b = el("button", ask ? "danger" : "", label);
    b.onclick = function () {
      var go = function () { body.car = d.index; send("/api/penalty", body, label + " queued for " + d.name); };
      if (ask) confirmBox(label + " for " + d.name + "?", go); else go();
    };
    return b;
  }

  function buildRow(d) {
    var tr = el("tr", d.in_pit ? "pit" : "");
    tr.dataset.car = d.index;
    var pos = el("td", "num", d.position || "-");
    var name = el("td", "drv" + (d.human ? " human" : ""), d.name);
    var team = el("td", "dim", d.team);
    var lap = el("td", "num", d.lap || "-");
    var pen = el("td", "num", d.penalty_s ? "+" + d.penalty_s + "s" : "");
    var warn = el("td", "num", d.warnings || "");
    var act = el("td");
    var box = el("div", "pen");
    box.appendChild(penaltyBtn(d, "Warning", { kind: "warning", reason: "Steward warning" }));
    box.appendChild(penaltyBtn(d, "+5s", { kind: "time", seconds: 5, reason: "Steward penalty" }));
    box.appendChild(penaltyBtn(d, "+10s", { kind: "time", seconds: 10, reason: "Steward penalty" }));
    box.appendChild(penaltyBtn(d, "Drive-through", { kind: "drive_through", reason: "Steward penalty" }));
    box.appendChild(penaltyBtn(d, "Stop-go", { kind: "stop_go", reason: "Steward penalty" }));
    box.appendChild(penaltyBtn(d, "DSQ", { kind: "dsq", reason: "Steward disqualification" }, true));
    var inp = el("input");
    inp.type = "number"; inp.min = "1"; inp.max = "255"; inp.placeholder = "sec";
    var go = el("button", "", "Add");
    go.onclick = function () {
      var s = parseInt(inp.value, 10);
      if (!(s > 0)) { toast("Enter seconds", true); return; }
      send("/api/penalty", { car: d.index, kind: "time", seconds: s, reason: "Steward penalty" }, "+" + s + "s queued for " + d.name);
      inp.value = "";
    };
    box.appendChild(inp);
    box.appendChild(go);
    act.appendChild(box);
    [pos, name, team, lap, pen, warn, act].forEach(function (c) { tr.appendChild(c); });
    return tr;
  }

  function renderGrid(drivers) {
    var key = drivers.map(function (d) { return [d.index, d.name, d.team, d.position, d.lap, d.in_pit, d.human, d.penalty_s, d.warnings].join(":"); }).join("|");
    $("grid-empty").hidden = drivers.length > 0;
    if (key === gridKey) return;
    var active = document.activeElement;
    if (active && active.tagName === "INPUT" && $("grid-body").contains(active)) {
      var cur = active.closest("tr").dataset.car;
      var inpVal = active.value;
      gridKey = key;
      patchRows(drivers);
      var tr = $("grid-body").querySelector('tr[data-car="' + cur + '"]');
      if (tr) { var i = tr.querySelector("input"); i.value = inpVal; i.focus(); }
      return;
    }
    gridKey = key;
    patchRows(drivers);
  }

  function patchRows(drivers) {
    var body = $("grid-body");
    body.textContent = "";
    drivers.forEach(function (d) { body.appendChild(buildRow(d)); });
  }

  function fmtT(t) {
    var s = Math.max(0, Math.floor(t));
    var m = Math.floor(s / 60);
    return m + ":" + (s % 60 < 10 ? "0" : "") + (s % 60);
  }

  function renderEvents(events) {
    var ul = $("events");
    ul.textContent = "";
    $("events-empty").hidden = events.length > 0;
    for (var i = events.length - 1; i >= 0; i--) {
      var e = events[i];
      var li = el("li");
      li.appendChild(el("span", "code " + e.code, e.code));
      li.appendChild(el("span", "", (e.t ? fmtT(e.t) + "  " : "") + e.text));
      ul.appendChild(li);
    }
  }

  function setPill(id, text, cls) {
    var p = $(id);
    p.textContent = text;
    p.className = "pill " + (cls || "");
  }

  function render(s) {
    role = s.role;
    setPill("pill-phase", s.phase, s.phase === "race" ? "on" : "");
    setPill("pill-track", s.track || "", "on");
    setPill("pill-lap", s.total_laps ? "Lap " + s.lap + " / " + s.total_laps : (s.lap ? "Lap " + s.lap : ""), "on");
    var rc = s.race_control || { state: "none" };
    setPill("pill-rc", RC_LABEL[rc.state] || rc.state, rc.state === "none" ? "" : (rc.state === "red_flag" ? "bad" : "warn"));
    var rs = $("rc-state");
    rs.textContent = RC_LABEL[rc.state] || rc.state;
    rs.className = "rc-state " + rc.state;
    var tele = s.telemetry || {};
    $("banner").hidden = !!tele.receiving;
    $("banner-port").textContent = tele.port || 20777;
    setPill("pill-conn", s.role === "host" ? "host" : s.role, s.role === "host" ? "good" : "warn");
    if (!autoBusy) $("auto").checked = !!s.auto_penalties;
    var host = s.role === "host";
    document.querySelectorAll("[data-rc], #auto").forEach(function (b) { b.disabled = !host; });
    document.querySelectorAll("#grid-body button").forEach(function (b) { b.disabled = !host; });
    renderGrid(s.drivers || []);
    if (!host) document.querySelectorAll("#grid-body button").forEach(function (b) { b.disabled = true; });
    renderEvents(s.events || []);
  }

  function poll() {
    api("/api/state").then(function (s) {
      failures = 0;
      $("offline").hidden = true;
      render(s);
    }, function () {
      if (++failures >= 3) $("offline").hidden = false;
    }).then(function () { setTimeout(poll, 500); });
  }

  document.querySelectorAll("[data-rc]").forEach(function (b) {
    b.onclick = function () {
      var a = b.getAttribute("data-rc");
      confirmBox(RC_ACTIONS[a], function () { send("/api/race-control", { action: a }, "Race control: " + a.replace("_", " ")); });
    };
  });

  $("auto").onchange = function () {
    var on = this.checked;
    autoBusy = true;
    api("/api/auto-penalties", { enabled: on }).then(function (r) {
      if (r && r.ok === false) toast(r.error || "Failed", true);
    }, function () { toast("No contact with the game", true); }).then(function () {
      setTimeout(function () { autoBusy = false; }, 800);
    });
  };

  poll();
})();
