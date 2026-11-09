// The chat view. The macOS shell calls window.kuterlapa.render(state) with
// what the core sends, and window.kuterlapa.scroll(...) for the pad.
//
// state = {
//   scale: 1.3,
//   session: "kuterlapa", label: "Kuterłapa", automatic: false,
//   conversation: { kind: "Rozmowa" | "Polecenie", index: 3, count: 5, created: "2026-11-08 19:57" },
//   other_session: false,  // the view shows another session than the current
//   focus: 4,              // a message to scroll to and mark, or null
//   sessions: [{ name, label, current, when }],
//   messages: [{ role: "user" | "assistant", text, profile, created }],
//   follow: true   // scroll to the newest message
// }

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

function message(m) {
  const div = document.createElement("div");
  div.className = `message ${m.role}`;
  const who = m.role === "user" ? "Ty" : m.profile || "Model";
  div.innerHTML = `<div class="meta">${escapeHtml(who)} · ${escapeHtml(m.created || "")}</div>` +
    `<div class="body">${renderMarkdown(m.text || "")}</div>`;
  div.querySelectorAll("pre code").forEach((block) => hljs.highlightElement(block));
  return div;
}

function render(state) {
  document.documentElement.style.setProperty("--scale", state.scale || 1);

  const list = $("session-list");
  list.innerHTML = "";
  for (const s of state.sessions || []) {
    const li = document.createElement("li");
    if (s.shown) li.className = s.current ? "current" : "shown";
    li.innerHTML = `<span class="name">${escapeHtml(s.label)}</span><span class="when">${escapeHtml(s.when || "")}</span>`;
    list.appendChild(li);
  }

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
  if (Number.isInteger(state.focus) && nodes[state.focus]) {
    // A message a model found: in the middle of the view, marked.
    const found = nodes[state.focus];
    found.classList.add("focus");
    box.style.scrollBehavior = "auto";
    box.scrollTop = found.offsetTop - box.offsetTop - box.clientHeight / 3;
    box.style.scrollBehavior = "";
  } else if (state.follow || atEnd) {
    box.style.scrollBehavior = "auto";
    box.scrollTop = box.scrollHeight;
    box.style.scrollBehavior = "";
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

window.kuterlapa = { render, scroll };
