// Shared by the chat site packs: lists of conversations, and finding one
// by a spoken name.

// Letters only, without accents: "Karolinka ❤️" and "karolinka" are equal.
function normName(s) {
  return (s || "").toLowerCase().replace(/ł/g, "l").normalize("NFD")
    .replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9]+/g, " ").trim();
}

// Spoken names differ from the contact's: "Karolineczka" for "Karolinka",
// a first name for a full one. The whole name scores above a single word
// of it. Answers {pick}, {choices} or {none}.
function pickByName(items, wanted) {
  const w = normName(wanted);
  const prefix = (a, b) => { let i = 0; while (i < a.length && a[i] === b[i]) i++; return i; };
  const score = (name) => {
    const whole = normName(name);
    if (!whole || !w) return 0;
    if (whole === w) return 4;
    if (whole.startsWith(w) || w.startsWith(whole)) return 3;
    if (whole.split(" ").includes(w)) return 2.5;
    let best = 0;
    const p = prefix(whole, w);
    if (p >= 4) best = 1 + p / 100;
    for (const word of whole.split(" ")) {
      const q = prefix(word, w);
      if (q >= 4) best = Math.max(best, 0.9 + q / 100);
    }
    return best;
  };
  const scored = items.map((item) => ({ item, s: score(item.name) })).filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s);
  if (!scored.length) return { none: true };
  const top = scored.filter((x) => x.s === scored[0].s);
  // Search results list one person several times (the chat, the contact,
  // a message): the same name is one choice.
  const names = [...new Set(top.map((x) => x.item.name))];
  if (names.length > 1) return { choices: names };
  return { pick: top[0].item };
}

function chatLines(items) {
  return items.map((c) => {
    const unread = c.unread ? ` (${c.unread} unread)` : "";
    const last = c.last ? ` | ${c.last.slice(0, 70)}` : "";
    return `${c.open ? "*" : " "} ${c.name}${unread}${last}`;
  }).join("\n") || "No conversations shown.";
}

// The middle of an element in screen points, for a real click. Chrome's
// outer size is in points and its inner size in CSS pixels, so their ratio
// is the page zoom; the difference in height is the toolbar.
function screenPoint(el) {
  const r = el.getBoundingClientRect();
  const zoom = window.outerWidth / window.innerWidth;
  const toolbar = window.outerHeight - window.innerHeight * zoom;
  return {
    x: Math.round(window.screenX + (r.left + r.width / 2) * zoom),
    y: Math.round(window.screenY + toolbar + (r.top + r.height / 2) * zoom),
  };
}

// The answer of list_chats and open_chat, the same for every chat site.
// open_chat asks for a real click (chrome.js does it): WhatsApp ignores
// mouse events made in the page. When the name is not in the list and the
// site can search (`search` types the name and returns true), it searches,
// and the next round picks from the results.
function chatAction(site, items, search) {
  if (action === "list_chats") return `${site}, the list on screen (* is open):\n${chatLines(items)}`;
  const found = pickByName(items, args.name);
  if (found.choices) {
    return { error: `more than one conversation fits "${args.name}": ${found.choices.join("; ")}. Ask the user which, then call open_chat with the exact name.` };
  }
  if (found.none) {
    if (search && !(result && result.searched) && search(args.name)) return { again: true, wait: 1.5, searched: true };
    const where = result && result.searched ? "in the search results" : "in the list on screen";
    return { error: `no conversation like "${args.name}" ${where}:\n${chatLines(items)}` };
  }
  found.pick.el.scrollIntoView({ block: "center" });
  return { site, opened: found.pick.name, click: screenPoint(found.pick.el) };
}

// The check after open_chat: does the page show that conversation now?
function chatCheck(shown) {
  if (normName(shown) === normName(result.opened)) return `Opened the conversation with ${result.opened} in ${result.site}.`;
  return { error: `clicked ${result.opened} in ${result.site}, but the page shows ${shown ? `"${shown}"` : "no conversation"}. Check with a screenshot.` };
}
