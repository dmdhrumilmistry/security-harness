// The terminals page: PTYs on the console's machine, one tab each, over
// WebSockets. Keystrokes go up as binary frames, resizes as
// {"t":"resize"} text frames; output comes back as binary frames, and a
// first {"t":"session","id":N} names the terminal on the server.
//
// Closing a tab ends its program (and what it started). The console's
// Terminal page opens tabs here when this page is already open (a
// BroadcastChannel), instead of another window.
(function () {
  "use strict";
  var KEYS = ["what", "h", "target", "proc", "profile"];

  function $(id) { return document.getElementById(id); }
  function h(tag, attrs, kids) {
    var e = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (k) {
      if (k === "text") e.textContent = attrs[k];
      else if (k.slice(0, 2) === "on") e.addEventListener(k.slice(2), attrs[k]);
      else e.setAttribute(k, attrs[k]);
    });
    (kids || []).forEach(function (c) { if (c) e.appendChild(typeof c === "string" ? document.createTextNode(c) : c); });
    return e;
  }
  function saved(k) { try { return localStorage.getItem(k); } catch (e) { return null; } }
  var t = saved("indra.theme");
  if (t === "light" || t === "dark") document.documentElement.setAttribute("data-theme", t);
  var dark = t === "dark" || (t !== "light" && window.matchMedia && matchMedia("(prefers-color-scheme: dark)").matches);
  var css = getComputedStyle(document.documentElement);
  function v(name, fallback) { return (css.getPropertyValue(name) || "").trim() || fallback; }

  // Open terminals once the symbols font is in, so its icons are measured
  // right (a second at most; without it the terminals still work).
  var fontReady = (document.fonts && document.fonts.load)
    ? Promise.race([document.fonts.load('13px "Symbols Nerd Font Mono"', ""), new Promise(function (r) { setTimeout(r, 1500); })]).catch(function () {})
    : Promise.resolve();

  var tabs = [], active = null, seq = 0;
  var enc = new TextEncoder();

  function paramsOf(src) {
    var p = new URLSearchParams();
    KEYS.forEach(function (k) { if (src[k] != null && src[k] !== "") p.set(k, String(src[k])); });
    if (!p.get("what")) p.set("what", "shell");
    return p;
  }

  function select(tab) {
    active = tab;
    tabs.forEach(function (x) {
      var on = x === tab;
      x.pane.hidden = !on;
      x.tabEl.classList.toggle("on", on);
      x.tabEl.setAttribute("aria-selected", on ? "true" : "false");
    });
    if (tab) {
      document.title = tab.label + " - indra";
      setTimeout(function () { try { tab.fit.fit(); } catch (e) { /* hidden */ } tab.term.focus(); }, 0);
    } else {
      document.title = "indra terminals";
    }
    $("none").hidden = tabs.length > 0;
  }

  function setState(tab, text, cls) {
    tab.dot.className = "tdot " + (cls || "");
    tab.dot.title = text;
    tab.tabEl.dataset.state = text;
  }

  function closeTab(tab, quiet) {
    if (!quiet && tab.ws && tab.ws.readyState <= 1 && !window.confirm("Close " + tab.label + "? Its program, and anything it started, ends.")) return;
    tab.closing = true;
    if (tab.ws) { try { tab.ws.close(); } catch (e) { /* gone */ } }
    try { tab.term.dispose(); } catch (e) { /* gone */ }
    tab.pane.remove(); tab.tabEl.remove();
    var i = tabs.indexOf(tab);
    tabs.splice(i, 1);
    if (active === tab) select(tabs[Math.min(i, tabs.length - 1)] || null);
    else $("none").hidden = tabs.length > 0;
  }

  function shortLabel(l) {
    // "shell in /home/me/code/api" -> "shell: api"
    var m = /^(.*?) in (.*)$/.exec(l || "");
    if (!m) return l;
    var dir = m[2].replace(/[\\/]+$/, "").split(/[\\/]/).pop();
    return m[1] + ": " + (dir || m[2]);
  }

  function connect(tab) {
    setState(tab, "starting", "");
    fetch("/api/term/check?" + tab.params.toString(), { credentials: "same-origin" })
      .then(function (r) { return r.json().then(function (j) { return { ok: r.ok, j: j }; }); })
      .then(function (r) {
        if (!r.ok) throw new Error(r.j.error || "refused");
        tab.label = r.j.label;
        tab.name.textContent = shortLabel(r.j.label);
        tab.tabEl.title = r.j.label;
        if (active === tab) document.title = tab.label + " - indra";
        var u = new URL("/api/term?" + tab.params.toString(), location.href);
        u.protocol = location.protocol === "https:" ? "wss:" : "ws:";
        var ws = new WebSocket(u.toString());
        tab.ws = ws;
        ws.binaryType = "arraybuffer";
        ws.onopen = function () {
          setState(tab, "connected", "ok");
          ws.send(JSON.stringify({ t: "resize", cols: tab.term.cols, rows: tab.term.rows }));
          if (active === tab) tab.term.focus();
        };
        ws.onmessage = function (e) {
          if (typeof e.data === "string") {
            if (e.data.charAt(0) === "{") {
              try { var m = JSON.parse(e.data); if (m && m.t === "session") { tab.sid = m.id; tab.tabEl.dataset.sid = String(m.id); tab.num.textContent = "#" + m.id; return; } } catch (x) { /* text output */ }
            }
            tab.term.write(e.data);
          } else {
            tab.term.write(new Uint8Array(e.data));
          }
        };
        ws.onclose = function () {
          if (tab.ws === ws) tab.ws = null;
          if (tab.closing) return;
          setState(tab, "ended", "warn");
          tab.term.write("\r\n\x1b[2m[ended]\x1b[0m\r\n");
          tab.again.hidden = false;
        };
      })
      .catch(function (e) {
        setState(tab, "unavailable", "bad");
        tab.term.write("\r\n" + String(e.message || e) + "\r\n");
        tab.again.hidden = false;
      });
  }

  function openTab(src) {
    var params = paramsOf(src);
    var what = params.get("what");
    var term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      // An installed Nerd Font first (they match its prompt exactly), then
      // the usual monospace, then the bundled symbols for any icon the text
      // font lacks.
      fontFamily: '"MesloLGS NF", "JetBrainsMono Nerd Font", "FiraCode Nerd Font", "Hack Nerd Font", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace, "Symbols Nerd Font Mono"',
      scrollback: 5000,
      theme: {
        background: v("--panel", dark ? "#1b1b1a" : "#ffffff"),
        foreground: v("--fg", dark ? "#ececea" : "#1c1c1a"),
        cursor: v("--accent", "#2f6fdf"),
        selectionBackground: dark ? "#3a4a6a" : "#cfdcf5"
      }
    });
    var fit = new FitAddon.FitAddon();
    term.loadAddon(fit);
    var tab = { id: ++seq, params: params, term: term, fit: fit, ws: null,
      label: what === "claude" ? "Claude Code" : what === "agent" ? "session" : "shell" };
    tab.dot = h("span", { class: "tdot" });
    tab.name = h("span", { class: "tname", text: tab.label });
    // The number the console's Running terminals list shows too.
    tab.num = h("span", { class: "tnum" });
    tab.tabEl = h("div", { class: "ttab", role: "tab", tabindex: "0",
      onclick: function () { select(tab); },
      onauxclick: function (e) { if (e.button === 1) { e.preventDefault(); closeTab(tab); } },
      onkeydown: function (e) { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); select(tab); } } },
      [tab.dot, tab.name, tab.num,
       h("button", { type: "button", class: "tclose", title: "Close this terminal", "aria-label": "Close terminal",
         onclick: function (e) { e.stopPropagation(); closeTab(tab); } }, ["×"])]);
    tab.again = h("button", { type: "button", class: "tagain", hidden: "",
      onclick: function () { tab.again.hidden = true; term.reset(); connect(tab); } }, ["Start again"]);
    tab.pane = h("div", { class: "termpane", role: "tabpanel" }, [tab.again]);
    $("tabs").appendChild(tab.tabEl);
    $("panes").appendChild(tab.pane);
    tabs.push(tab);
    term.open(tab.pane);
    term.onData(function (d) { if (tab.ws && tab.ws.readyState === 1) tab.ws.send(enc.encode(d)); });
    term.onBinary(function (d) {
      var b = new Uint8Array(d.length);
      for (var i = 0; i < d.length; i++) b[i] = d.charCodeAt(i) & 255;
      if (tab.ws && tab.ws.readyState === 1) tab.ws.send(b);
    });
    term.onResize(function (sz) { if (tab.ws && tab.ws.readyState === 1) tab.ws.send(JSON.stringify({ t: "resize", cols: sz.cols, rows: sz.rows })); });
    select(tab);
    try { fit.fit(); } catch (e) { /* hidden */ }
    connect(tab);
    return tab;
  }

  window.addEventListener("resize", function () { if (active) { try { active.fit.fit(); } catch (e) { /* hidden */ } } });
  // Leaving the page ends its terminals: say so first.
  window.addEventListener("beforeunload", function (e) {
    if (tabs.some(function (x) { return x.ws && x.ws.readyState === 1; })) { e.preventDefault(); e.returnValue = ""; }
  });
  // Ctrl+Shift+T opens, Ctrl+Shift+W closes, Ctrl+Shift+PageUp/Down switch.
  window.addEventListener("keydown", function (e) {
    if (!(e.ctrlKey && e.shiftKey)) return;
    var k = e.key.toUpperCase ? e.key.toUpperCase() : e.key;
    if (k === "T") { e.preventDefault(); showNew(true); }
    else if (k === "W" && active) { e.preventDefault(); closeTab(active); }
    else if ((e.key === "PageDown" || e.key === "PageUp") && tabs.length > 1) {
      e.preventDefault();
      var i = tabs.indexOf(active) + (e.key === "PageDown" ? 1 : -1);
      select(tabs[(i + tabs.length) % tabs.length]);
    }
  }, true);

  // The New form: a shell or Claude Code (with a profile) in a target.
  var ov = null;
  function showNew(on) {
    $("newform").hidden = !on;
    $("new").setAttribute("aria-expanded", on ? "true" : "false");
    if (on) $("nwhat").focus();
    else if (active) active.term.focus();
  }
  function fillNew(hname) {
    var hs = (ov.harnesses || []).find(function (x) { return x.name === hname; }) || (ov.harnesses || [])[0];
    var ts = $("ntarget"); ts.textContent = "";
    var targets = hs ? ((hs.targets && hs.targets.length) ? hs.targets : [hs.root]) : [];
    targets.forEach(function (tg, i) { ts.appendChild(h("option", { value: String(i), text: tg })); });
    ts.dataset.h = hs ? hs.name : "";
    var ps = $("nprofile"); ps.textContent = "";
    ps.appendChild(h("option", { value: "", text: "no profile" }));
    (ov.profiles || []).filter(function (p) { return !p.runner || p.runner === "claude"; }).forEach(function (p) {
      ps.appendChild(h("option", { value: p.name, text: p.name + (p.provider === "ollama" ? " (Ollama: " + (p.model || "?") + ")" : "") }));
    });
  }
  $("nwhat").addEventListener("change", function () { $("nprofilebox").hidden = $("nwhat").value !== "claude"; });
  $("new").addEventListener("click", function () { showNew($("newform").hidden); });
  $("ncancel").addEventListener("click", function () { showNew(false); });
  $("none-new").addEventListener("click", function () { showNew(true); });
  $("newform").addEventListener("submit", function (e) {
    e.preventDefault();
    var src = { what: $("nwhat").value, h: $("ntarget").dataset.h, target: $("ntarget").value };
    if (src.what === "claude" && $("nprofile").value) src.profile = $("nprofile").value;
    showNew(false);
    openTab(src);
  });

  // The console asks this page to open a tab (instead of a new window).
  if (window.BroadcastChannel) {
    var bc = new BroadcastChannel("indra-term");
    bc.onmessage = function (e) {
      var m = e.data || {};
      if (m.t === "ping") bc.postMessage({ t: "pong", n: m.n });
      if (m.t === "open" && m.params) { openTab(m.params); try { window.focus(); } catch (x) { /* blocked */ } }
      if (m.t === "closed" && m.id != null) {
        // The console closed a terminal: drop its tab here too.
        tabs.filter(function (x) { return x.sid === m.id; }).forEach(function (x) { closeTab(x, true); });
      }
    };
  }

  var q = new URLSearchParams(location.search), first = {};
  KEYS.forEach(function (k) { if (q.get(k) !== null) first[k] = q.get(k); });

  fetch("/api/overview", { credentials: "same-origin" }).then(function (r) { return r.ok ? r.json() : null; }).then(function (o) {
    if (o) { ov = o; fillNew(first.h || saved("indra.h")); }
  }).catch(function () { /* the New form stays empty */ });

  fontReady.then(function () {
    // Opened with what to start: start it. Opened bare: offer the New form.
    if (first.what) openTab(first);
    else { $("none").hidden = false; showNew(true); }
    // The address bar no longer names one terminal (a reload must not
    // start it again).
    if (history.replaceState) history.replaceState(null, "", "/term");
  });
})();
