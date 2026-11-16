// The text of a row starts with "Active now" for people online; the name
// and the last message are its dir=auto spans.
const items = [...document.querySelectorAll("[role=row] a[href]")]
  .filter((a) => /\/t\/[^/]+/.test(a.getAttribute("href")))
  .map((a) => {
    const spans = [...a.querySelectorAll("span[dir=auto]")].map((s) => s.innerText.trim()).filter(Boolean);
    return { el: a, name: spans[0] || "", last: spans.slice(1, 3).join(" · "), unread: "",
      open: a.getAttribute("aria-current") === "page" };
  })
  .filter((i) => i.name);
return chatAction("Messenger", items);
