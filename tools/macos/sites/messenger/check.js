const link = document.querySelector('[role=row] a[aria-current="page"]');
const span = link && link.querySelector("span[dir=auto]");
return chatCheck(span ? span.innerText.trim() : "");
