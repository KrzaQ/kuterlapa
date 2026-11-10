// Google Chrome for Kuterlapa's model tools, through JavaScript for
// Automation. Run: osascript -l JavaScript chrome.js <command> [args]
//
//   tabs                       windows and their tabs; * marks the active tab
//   tab <window> <tab>         bring that tab to the front
//   close <window> <tab>...    close tabs of one window (cmd+shift+t in
//                              Chrome brings a tab back)
//   open <url> [profile] [new|tab] go to the site's open tab if there is
//                              one, else open the address in a new tab of
//                              the front window; with a profile name, in a
//                              window of that profile; "new": in a new
//                              window; "tab": a new tab in any case
//   profiles                   the names of Chrome's profiles
//
// Pages, through JavaScript in the tab (Chrome: View, Developer, Allow
// JavaScript from Apple Events must be on). Window and tab 0 mean the front
// window and its active tab:
//
//   read <w> <t> [chars]       the page's title, address and text
//   fields <w> <t>             its fields, buttons and links, numbered
//   fill <w> <t> <n> <text> [submit]
//                              put text into field n (as typing would), and
//                              with "submit" send its form
//   click <w> <t> <n>          click element n
//
// computer_windows shows which profile each Chrome window belongs to.

ObjC.import("Foundation");

function chrome() {
	return Application("Google Chrome");
}

function profiles() {
	const path = $("~/Library/Application Support/Google/Chrome/Local State").stringByExpandingTildeInPath.js;
	const text = $.NSString.stringWithContentsOfFileEncodingError(path, $.NSUTF8StringEncoding, null);
	if (!text) return {};
	const cache = JSON.parse(text.js).profile.info_cache;
	const byName = {};
	for (const dir in cache) byName[cache[dir].name] = dir;
	return byName;
}

function shell(command) {
	const app = Application.currentApplication();
	app.includeStandardAdditions = true;
	return app.doShellScript(command);
}

function quote(s) {
	return "'" + String(s).replace(/'/g, "'\\''") + "'";
}

function tabAt(c, windowNumber, tabNumber) {
	const w = Number(windowNumber) > 0 ? c.windows[Number(windowNumber) - 1] : c.windows[0];
	return Number(tabNumber) > 0 ? w.tabs[Number(tabNumber) - 1] : w.activeTab();
}

/** Runs JavaScript in a tab; the page's answer is a JSON string. */
function inPage(tab, code) {
	try {
		const answer = tab.execute({ javascript: code });
		return answer === undefined || answer === null ? "" : String(answer);
	} catch (e) {
		if (String(e).includes("turned off") || String(e).includes("Apple Events")) {
			return "error: Chrome does not allow JavaScript from Apple Events. The user turns it on in Chrome: View, Developer, Allow JavaScript from Apple Events.";
		}
		return `error: ${e}`;
	}
}

// Numbers the visible fields, buttons and links with a data attribute, so
// fill and click find them again. Inside the page, as one expression.
// Numbers from an earlier list go first: a hidden element that kept one
// would otherwise be found instead of the visible one.
const MARK = `(() => {
  document.querySelectorAll("[data-kuterlapa]").forEach((el) => el.removeAttribute("data-kuterlapa"));
  const seen = (el) => { const r = el.getBoundingClientRect(); const st = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && st.visibility !== "hidden" && st.display !== "none"; };
  const label = (el) => (el.getAttribute("aria-label") || el.labels?.[0]?.innerText || el.placeholder ||
    el.title || el.name || el.innerText || el.value || "").replace(/\\s+/g, " ").trim().slice(0, 80);
  const all = [...document.querySelectorAll(
    "input:not([type=hidden]), textarea, select, button, [contenteditable=true], [role=button], [role=textbox], a[href]")]
    .filter(seen).slice(0, 150);
  return JSON.stringify(all.map((el, i) => { el.setAttribute("data-kuterlapa", String(i + 1));
    const kind = el.tagName.toLowerCase() + (el.type && el.tagName === "INPUT" ? ":" + el.type : "") +
      (el.isContentEditable ? ":editable" : "");
    return "[" + (i + 1) + "] " + kind + " " + JSON.stringify(label(el)) +
      (el.value && el.tagName !== "BUTTON" && el.type !== "password" ? " = " + JSON.stringify(String(el.value).slice(0, 60)) : ""); }));
})()`;

// Finds element n of the last list, inside the page: the visible one, or
// an error to return.
const FIND = (n) => `
  const seen = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const marked = [...document.querySelectorAll('[data-kuterlapa="${Number(n)}"]')];
  const el = marked.find(seen);
  if (!el) return marked.length ? "error: element ${Number(n)} is not visible any more; list the fields again"
                                 : "error: no element ${Number(n)}; list the fields first";
  const name = (el.getAttribute("aria-label") || el.labels?.[0]?.innerText || el.placeholder || el.name || "")
    .replace(/\\s+/g, " ").trim().slice(0, 60);`;

function run(argv) {
	const [command, ...args] = argv;
	const c = chrome();
	switch (command) {
		case "tabs": {
			if (!c.running()) return "Chrome is not running.";
			const out = [];
			c.windows().forEach((w, i) => {
				const active = w.activeTabIndex();
				const tabs = w.tabs();
				out.push(`Window ${i + 1}: "${w.name()}" (${tabs.length} tabs)`);
				tabs.forEach((t, j) => {
					const mark = j + 1 === active ? "*" : " ";
					out.push(`${mark} [${i + 1}.${j + 1}] ${t.title().slice(0, 80)} | ${t.url().slice(0, 100)}`);
				});
			});
			return out.join("\n") || "No windows.";
		}
		case "tab": {
			const w = c.windows[Number(args[0]) - 1];
			w.activeTabIndex = Number(args[1]);
			w.index = 1;
			c.activate();
			return `Tab ${args[0]}.${args[1]}: ${w.activeTab().title()}`;
		}
		case "close": {
			const w = c.windows[Number(args[0]) - 1];
			// Close from the highest number down, so the numbers of the
			// tabs still to close stay right.
			// The numbers may come as one argument: "2 3 4" or "2,3,4".
			const tabs = args.slice(1).join(" ").split(/[\s,]+/).map(Number).filter((n) => n > 0).sort((a, b) => b - a);
			const closed = [];
			for (const n of tabs) {
				const tab = w.tabs[n - 1];
				closed.push(`${args[0]}.${n} ${tab.title()}`);
				tab.close();
			}
			return closed.length ? `Closed:\n${closed.join("\n")}` : "No tabs given.";
		}
		case "open": {
			const [url, profile, how] = args;
			if (profile) {
				const dir = profiles()[profile];
				if (!dir) return `error: no profile "${profile}"; profiles: ${Object.keys(profiles()).join(", ")}`;
				const extra = how === "new" ? " --new-window" : "";
				shell(`open -na "Google Chrome" --args --profile-directory=${quote(dir)}${extra} ${quote(url)}`);
				return `Opened in profile ${profile}.`;
			}
			// The site is open already: go to its tab instead of opening another
			// one, and load the address there if it asks for a page of the site.
			if (how !== "new" && how !== "tab") {
				const host = (u) => { const m = /^[a-z]+:\/\/(?:www\.)?([^/?#]+)/i.exec(u || ""); return m ? m[1].toLowerCase() : ""; };
				const path = (u) => (/^[a-z]+:\/\/[^/?#]+([^?#]*)/i.exec(u || "") || [])[1] || "/";
				const wanted = host(url);
				const windows = c.windows();
				for (let i = 0; wanted && i < windows.length; i++) {
					const tabs = windows[i].tabs();
					for (let j = 0; j < tabs.length; j++) {
						const current = tabs[j].url();
						if (host(current) !== wanted) continue;
						windows[i].activeTabIndex = j + 1;
						windows[i].index = 1;
						c.activate();
						const page = path(url).replace(/\/$/, "");
						if (page && page !== path(current).replace(/\/$/, "")) {
							tabs[j].url = url;
							return `The site was open already: went to tab ${i + 1}.${j + 1} and loaded ${url} there.`;
						}
						return `The site was open already: went to tab ${i + 1}.${j + 1} (${tabs[j].title().slice(0, 60)}).`;
					}
				}
			}
			if (how === "new" || c.windows().length === 0) {
				const w = c.Window().make();
				w.activeTab.url = url;
			} else {
				c.windows[0].tabs.push(c.Tab({ url }));
			}
			c.activate();
			return "Opened.";
		}
		case "profiles":
			return Object.keys(profiles()).join("\n");
		case "read": {
			const tab = tabAt(c, args[0], args[1]);
			const limit = Math.min(Number(args[2]) || 6000, 30000);
			const text = inPage(tab, `document.body ? document.body.innerText.slice(0, ${limit}) : ""`);
			if (text.startsWith("error:")) return text;
			return `${tab.title()}\n${tab.url()}\n\n${text}`;
		}
		case "fields": {
			const tab = tabAt(c, args[0], args[1]);
			const answer = inPage(tab, MARK);
			if (answer.startsWith("error:")) return answer;
			const list = JSON.parse(answer || "[]");
			return `${tab.title()}\n` + (list.length ? list.join("\n") : "No fields, buttons or links.");
		}
		case "fill": {
			const [w, t, n, text, submit] = args;
			const tab = tabAt(c, w, t);
			const code = `(() => {${FIND(n)}
  if (el.type === "password") return "error: Kuterlapa does not fill passwords";
  if (["checkbox", "radio", "button", "submit", "reset", "image", "file", "color", "range"].includes(el.type) ||
      ["BUTTON", "A"].includes(el.tagName))
    return "error: element ${Number(n)} (" + (el.type || el.tagName.toLowerCase()) + " " + JSON.stringify(name) + ") takes no text; use chrome_click";
  el.focus();
  const text = ${JSON.stringify(text || "")};
  if (el.isContentEditable) { document.execCommand("selectAll"); document.execCommand("insertText", false, text); }
  else if (el.tagName === "SELECT") {
    const option = [...el.options].find((o) => o.value === text || o.text.trim() === text);
    if (!option) return "error: " + JSON.stringify(name) + " has no option " + JSON.stringify(text);
    el.value = option.value;
    el.dispatchEvent(new Event("change", { bubbles: true }));
  } else {
    // As if typed: select all and insert, which fires the events that
    // frameworks listen to. If the page keeps the old text, the native
    // setter, which React and others notice too.
    el.select?.();
    document.execCommand("insertText", false, text);
    if (el.value !== text) {
      const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, "value").set.call(el, text);
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    }
  }
  if (${submit === "submit"}) {
    if (el.form) { el.form.requestSubmit ? el.form.requestSubmit() : el.form.submit(); }
    else { for (const type of ["keydown", "keypress", "keyup"]) el.dispatchEvent(new KeyboardEvent(type, { key: "Enter", code: "Enter", keyCode: 13, bubbles: true })); }
  }
  return "ok";
})()`;
			const answer = inPage(tab, code);
			if (answer !== "ok") return answer;
			if (submit === "submit") return `Filled ${n} and sent the form.`;
			// Read the field back a moment later: a page can change it again.
			delay(0.4);
			const shown = inPage(tab, `(() => {${FIND(n)}
  const value = el.isContentEditable ? el.innerText : el.tagName === "SELECT" ? el.options[el.selectedIndex]?.text : el.value;
  return JSON.stringify({ name, value: String(value ?? "") });
})()`);
			if (shown.startsWith("error")) return `Filled ${n}, but then: ${shown}`;
			const { name, value } = JSON.parse(shown);
			// Pages often add spaces (a phone number in groups): compare without them.
			const bare = (x) => String(x || "").replace(/\s+/g, "");
			if (bare(value) !== bare(text)) {
				return `error: field ${n} (${JSON.stringify(name)}) shows ${JSON.stringify(value)}, not the text; the page did not take it or changed it`;
			}
			return `Filled ${n} (${JSON.stringify(name)}): it shows ${JSON.stringify(value)}.`;
		}
		case "click": {
			const tab = tabAt(c, args[0], args[1]);
			const n = Number(args[2]);
			const answer = inPage(tab, `(() => {${FIND(n)}
  el.scrollIntoView({ block: "center" }); el.click(); return "ok: " + JSON.stringify(name || el.innerText?.trim().slice(0, 60) || ""); })()`);
			return answer.startsWith("ok") ? `Clicked ${n} ${answer.slice(4)}.` : answer;
		}
		default:
			return "usage: chrome.js tabs | tab <w> <t> | close <w> <t>... | open <url> [profile] [new|tab] | profiles | read <w> <t> [chars] | fields <w> <t> | fill <w> <t> <n> <text> [submit] | click <w> <t> <n>";
	}
}
