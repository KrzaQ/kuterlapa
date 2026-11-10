// The chat view. The macOS shell calls window.kuterlapa.render(state) with
// what the core sends, and window.kuterlapa.scroll(...) for the pad.
//
// state = {
//   scale: 1.3,
//   session: "kuterlapa", label: "Kuterłapa", automatic: false,
//   conversation: { kind: "Rozmowa" | "Polecenie", index: 3, count: 5, created: "2026-11-08 19:57" },
//   other_session: false,  // the view shows another session than the current
//   focus: 4,              // a message to scroll to and mark, or null
//   sessions: [{ name, label, current, shown, when }],
//   items: [{ kind, title, created, current }],  // the session's, newest first
//   messages: [{ role: "user" | "assistant", text, profile, created, work }],
//   follow: true   // scroll to the newest message
// }
//
// work = {                       // what the model did for an answer
//   input: 15500, output: 254,   // tokens read (all calls) and written
//   seconds: 7.2,
//   steps: [{ name, detail, state: "running" | "done" | "failed", result }],
//   context: 28100, window: 1000000,  // how full the context window is
//   limits: [{ name: "5h", used: 0.14, left: "1h20m", pace: 28 }],  // Claude's plan
//   thinking: "...",
//   profile: "Qwen 96k"          // only while it works
// }
//
// window.kuterlapa.work(work) shows the work of the answer that is coming,
// below the last message; work(null) removes it.

"use strict";

marked.setOptions({ gfm: true, breaks: true });

const $ = (id) => document.getElementById(id);

function escapeHtml(text) {
  return text.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

// Math first: marked would eat the backslashes and underscores of LaTeX.
// Each formula becomes a placeholder, marked renders the rest, and KaTeX
// fills the placeholders.
function renderMarkdown(text) {
  const formulas = [];
  const keep = (tex, display) => {
    formulas.push({ tex, display });
    return `@@MATH${formulas.length - 1}@@`;
  };
  let source = text
    .replace(/\$\$([\s\S]+?)\$\$/g, (_, tex) => keep(tex, true))
    .replace(/\\\[([\s\S]+?)\\\]/g, (_, tex) => keep(tex, true))
    .replace(/\\\(([\s\S]+?)\\\)/g, (_, tex) => keep(tex, false))
    .replace(/(^|[^\\\w$])\$([^\s$](?:[^$\n]*?[^\s$])?)\$(?![\w$])/g, (_, before, tex) => before + keep(tex, false));
  let html = marked.parse(source);
  html = html.replace(/@@MATH(\d+)@@/g, (_, i) => {
    const f = formulas[Number(i)];
    try {
      return katex.renderToString(f.tex, { displayMode: f.display, throwOnError: false });
    } catch (e) {
      return escapeHtml(f.tex);
    }
  });
  return html;
}

// The answers whose work the user unfolded, so a redraw keeps them open.
const unfolded = new Set();

function message(m) {
  const div = document.createElement("div");
  div.className = `message ${m.role}`;
  const who = m.role === "user" ? "Ty" : m.profile || "Model";
  // What the model did comes before its answer, folded: the summary line
  // shows, the tool calls and the thinking open with a click.
  const work = m.work
    ? `<details class="work done"${unfolded.has(m.id) ? " open" : ""}><summary>${summaryHtml(m.work)}</summary>` +
      stepsHtml(m.work) + thinkingHtml(m.work) + "</details>"
    : "";
  div.innerHTML = `<div class="meta">${escapeHtml(who)} · ${escapeHtml(m.created || "")}</div>` +
    work + `<div class="body">${renderMarkdown(m.text || "")}</div>`;
  const details = div.querySelector("details");
  if (details) details.addEventListener("toggle", () => details.open ? unfolded.add(m.id) : unfolded.delete(m.id));
  div.querySelectorAll("pre code").forEach((block) => hljs.highlightElement(block));
  return div;
}

// 537, 4,2k, 19,8k, 250k, 1,2M
function tokens(n) {
  if (n < 1000) return String(n);
  if (n < 100000) return (n / 1000).toFixed(1).replace(".", ",") + "k";
  if (n < 1000000) return Math.floor(n / 1000) + "k";
  return (n / 1000000).toFixed(1).replace(".0", "").replace(".", ",") + "M";
}

// "1 wywołanie", "3 wywołania", "5 wywołań"
function calls(n) {
  const word = n === 1 ? "wywołanie"
    : [2, 3, 4].includes(n % 10) && ![12, 13, 14].includes(n % 100) ? "wywołania" : "wywołań";
  return `${n} ${word}`;
}

// While the model works: everything, open.
function workHtml(w, live) {
  return summaryHtml(w, live) + stepsHtml(w) + (live ? thinkingHtml(w) : "");
}

function summaryHtml(w, live) {
  const steps = w.steps || [];
  const failed = steps.filter((s) => s.state === "failed").length;
  // Some engines count the tokens read only at the end of a call.
  const input = live && !w.input ? "…" : tokens(w.input || 0);
  const parts = [`<span class="tokens">${input}</span> wej.`,
    `<span class="tokens">${tokens(w.output || 0)}</span> wyj.`];
  if (w.context) {
    const share = w.window ? ` (${Math.round((w.context * 100) / w.window)}%)` : "";
    parts.push(`kontekst <span class="tokens">${tokens(w.context)}${w.window ? "/" + tokens(w.window) : ""}</span>${share}`);
  }
  if (steps.length) parts.push(`🔧 ${calls(steps.length)}` + (failed ? ` <span class="failed">(${failed} ✗)</span>` : ""));
  parts.push(live ? '<span class="seconds"></span>' : `${Math.round(w.seconds || 0)} s`);
  let html = `<div class="summary">${parts.join(" · ")}</div>`;
  // Claude's plan limits, as in the Claude Code status line.
  if ((w.limits || []).length) {
    const limit = (l) => {
      const tone = l.pace >= 100 ? "over" : l.pace >= 90 ? "near" : "fine";
      const extra = l.name === "5h" ? l.left : `${l.pace}%`;
      return `<span class="limit ${tone}">${escapeHtml(l.name)} ${Math.round(l.used * 100)}% (${escapeHtml(extra)})</span>`;
    };
    html += `<div class="summary">${w.limits.map(limit).join(" · ")}</div>`;
  }
  return html;
}

function stepsHtml(w) {
  const steps = w.steps || [];
  let html = "";
  if (steps.length) {
    html += '<ul class="steps">' + steps.map((s) => {
      const mark = { done: "✓", failed: "✗" }[s.state] || "⋯";
      return `<li class="${escapeHtml(s.state)}"><span class="mark">${mark}</span>` +
        `<span class="name">${escapeHtml(s.name)}</span>` +
        (s.detail ? ` <span class="args">${escapeHtml(s.detail)}</span>` : "") +
        (s.result ? ` <span class="result">→ ${escapeHtml(s.result)}</span>` : "") + "</li>";
    }).join("") + "</ul>";
  }
  return html;
}

function thinkingHtml(w) {
  return w.thinking ? `<div class="thinking"><div>💭 ${escapeHtml(w.thinking)}</div></div>` : "";
}

// The answer that is coming: the model's work, live.
let live = null;
let liveStart = 0;

function drawLive() {
  const box = $("messages");
  const atEnd = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
  const old = $("live");
  if (old) old.remove();
  if (live) {
    const div = document.createElement("div");
    div.id = "live";
    div.className = "message assistant working";
    div.innerHTML = `<div class="meta">${escapeHtml(live.profile || "Model")} · pracuje…</div>` +
      `<div class="work">${workHtml(live, true)}</div>`;
    box.appendChild(div);
    const thinking = div.querySelector(".thinking");
    if (thinking && thinking.scrollHeight > thinking.clientHeight + 2) thinking.classList.add("clipped");
    tick();
  }
  if (atEnd) {
    box.style.scrollBehavior = "auto";
    box.scrollTop = box.scrollHeight;
    box.style.scrollBehavior = "";
  }
}

function tick() {
  const el = document.querySelector("#live .seconds");
  if (el) el.textContent = `${Math.floor((Date.now() - liveStart) / 1000)} s`;
}
setInterval(tick, 500);

function work(state) {
  live = state;
  if (state) liveStart = Date.now() - (state.seconds || 0) * 1000;
  drawLive();
}

function render(state) {
  document.documentElement.style.setProperty("--scale", state.scale || 1);
  // An empty dark screen, behind the overlay for its screenshots.
  document.body.classList.toggle("backdrop", !!state.backdrop);

  const list = $("session-list");
  list.innerHTML = "";
  for (const s of state.sessions || []) {
    const li = document.createElement("li");
    if (s.shown) li.className = s.current ? "current" : "shown";
    li.innerHTML = `<span class="name">${escapeHtml(s.label)}</span><span class="when">${escapeHtml(s.when || "")}</span>`;
    list.appendChild(li);
  }

  const items = $("item-list");
  items.innerHTML = "";
  for (const it of state.items || []) {
    const li = document.createElement("li");
    li.className = (it.current ? "current " : "") + (it.kind === "Polecenie" ? "command" : "conversation");
    const day = (it.created || "").slice(5);
    li.innerHTML = `<span class="title">${escapeHtml(it.title || "(bez pytania)")}</span>` +
      `<span class="when">${escapeHtml(it.kind)} · ${escapeHtml(day)}</span>`;
    items.appendChild(li);
  }
  const current = items.querySelector("li.current");
  if (current) current.scrollIntoView({ block: "nearest" });

  const c = state.conversation || {};
  $("title").textContent = (state.label || state.session || "") + (state.other_session ? " (inna sesja)" : "");
  const mode = state.automatic ? " · sesja z okna" : "";
  $("where").textContent = c.count
    ? `${c.kind} · ${c.index} z ${c.count} · od ${c.created || "?"}${mode}`
    : `Brak rozmów${mode}`;

  const box = $("messages");
  const atEnd = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
  box.innerHTML = "";
  if (!(state.messages || []).length) {
    box.innerHTML = '<div class="empty">Tu pojawi się rozmowa. Naciśnij Y i mów.</div>';
  }
  const nodes = (state.messages || []).map(message);
  nodes.forEach((n) => box.appendChild(n));
  if (live) drawLive();
  if (Number.isInteger(state.focus) && nodes[state.focus]) {
    // A message a model found: in the middle of the view, marked.
    const found = nodes[state.focus];
    found.classList.add("focus");
    box.style.scrollBehavior = "auto";
    box.scrollTop = found.offsetTop - box.offsetTop - box.clientHeight / 3;
    box.style.scrollBehavior = "";
  } else if (state.follow || atEnd) {
    // Again once the page has laid out (fonts, formulas): right after the
    // app starts, the first render comes before that.
    const toEnd = () => {
      box.style.scrollBehavior = "auto";
      box.scrollTop = box.scrollHeight;
      box.style.scrollBehavior = "";
    };
    toEnd();
    requestAnimationFrame(toEnd);
    setTimeout(toEnd, 300);
  }
}

// how: "page-up", "page-down", "top", "bottom", or a number of pixels
// (positive scrolls down).
function scroll(how) {
  const box = $("messages");
  const page = box.clientHeight * 0.85;
  if (how === "page-up") box.scrollBy({ top: -page });
  else if (how === "page-down") box.scrollBy({ top: page });
  else if (how === "top") box.scrollTo({ top: 0 });
  else if (how === "bottom") box.scrollTo({ top: box.scrollHeight });
  else {
    box.style.scrollBehavior = "auto";
    box.scrollBy({ top: Number(how) || 0 });
    box.style.scrollBehavior = "";
  }
}

window.kuterlapa = { render, scroll, work };
