// Each row of the chat list holds two titles: the name, then the last
// message.
const items = [...document.querySelectorAll("#pane-side [role=row]")].map((row) => {
  const titles = [...row.querySelectorAll("span[title]")];
  const unread = [...row.querySelectorAll("[aria-label]")].map((e) => e.getAttribute("aria-label"))
    .find((l) => /unread/i.test(l)) || "";
  return {
    el: row.querySelector("[tabindex]") || row,
    name: titles[0]?.getAttribute("title") || "",
    last: titles[1]?.getAttribute("title") || "",
    unread: (unread.match(/\d+/) || [""])[0],
    open: !!row.querySelector("[aria-selected=true]"),
  };
}).filter((i) => i.name);
// WhatsApp opens a chat on mouse down, not on a plain click event.
return chatAction("WhatsApp", items, (el) => {
  for (const type of ["mousedown", "mouseup", "click"])
    el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
});
