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
  if (top.length > 1) return { choices: top.map((x) => x.item.name) };
  return { pick: top[0].item };
}

function chatLines(items) {
  return items.map((c) => {
    const unread = c.unread ? ` (${c.unread} unread)` : "";
    const last = c.last ? ` | ${c.last.slice(0, 70)}` : "";
    return `${c.open ? "*" : " "} ${c.name}${unread}${last}`;
  }).join("\n") || "No conversations shown.";
}

// The answer of list_chats and open_chat, the same for every chat site.
function chatAction(site, items, open) {
  if (action === "list_chats") return `${site}, the list on screen (* is open):\n${chatLines(items)}`;
  const found = pickByName(items, args.name);
  if (found.choices) {
    return { error: `more than one conversation fits "${args.name}": ${found.choices.join("; ")}. Ask the user which, then call open_chat with the exact name.` };
  }
  if (found.none) {
    return { error: `no conversation like "${args.name}" in the list on screen:\n${chatLines(items)}\nIf the person is not there, search with chrome_fields and chrome_fill.` };
  }
  found.pick.el.scrollIntoView({ block: "center" });
  open(found.pick.el);
  return { site, opened: found.pick.name };
}

// The check after open_chat: does the page show that conversation now?
function chatCheck(shown) {
  if (normName(shown) === normName(result.opened)) return `Opened the conversation with ${result.opened} in ${result.site}.`;
  return { error: `clicked ${result.opened} in ${result.site}, but the page shows ${shown ? `"${shown}"` : "no conversation"}. Check with a screenshot.` };
}
