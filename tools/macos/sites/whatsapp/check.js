const header = document.querySelector("#main header span[title], #main header [dir=auto]");
return chatCheck(header ? header.getAttribute("title") || header.innerText : "");
