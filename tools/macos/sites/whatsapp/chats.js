// Each row of the chat list holds two titles: the name, then the last
// message.
const items = [...document.querySelectorAll("#pane-side [role=row]")].map((row) => {
  const titles = [...row.querySelectorAll("span[title]")];
  const unread = [...row.querySelectorAll("[aria-label]")].map((e) => e.getAttribute("aria-label"))
    .find((l) => /unread/i.test(l)) || "";
  return {
    // The name: a click on the row's text opens the chat.
    el: titles[0] || row,
    name: titles[0]?.getAttribute("title") || "",
    last: titles[1]?.getAttribute("title") || "",
    unread: (unread.match(/\d+/) || [""])[0],
    open: !!row.querySelector("[aria-selected=true]"),
  };
}).filter((i) => i.name);
// The search box above the list: typed text filters chats, contacts and
// messages, and the list shows the results.
function search(name) {
  const box = document.querySelector('#side input[type="text"], #side [contenteditable="true"]');
  if (!box) return false;
  box.focus();
  // As if typed: select what is there and insert, which fires the events
  // the page listens to.
  if (box.select) box.select(); else document.execCommand("selectAll");
  document.execCommand("insertText", false, name);
  return true;
}
return chatAction("WhatsApp", items, search);
