// Google Chrome for Kuterlapa's model tools, through JavaScript for
// Automation. Run: osascript -l JavaScript chrome.js <command> [args]
//
//   tabs                       windows and their tabs; * marks the active tab
//   tab <target>               bring that tab, its window and Chrome to the
//                              front
//   close <window> <tab>...    close tabs of one window (cmd+shift+t in
//                              Chrome brings a tab back)
//   open <url> [profile] [new|tab] open the address in a new tab of the
//                              front window, or only list the site's tabs
//                              when it is open already; with a profile
//                              name, in a window of that profile; "new":
//                              in a new window; "tab": a new tab in any case
//   profiles                   the names of Chrome's profiles
//
// Pages, through JavaScript in the tab (Chrome: View, Developer, Allow
// JavaScript from Apple Events must be on). A target is "window.tab" from
// `tabs`, for example "2.3". An empty target means the front window's active
// tab: the one the user sees. Models work there unless the user named
// another tab.
//
//   read <target> [chars]      the page's title, address and text
//   fields <target>            its fields, buttons and links, numbered
//   fill <target> <n> <text> [submit]
//                              put text into field n (as typing would), and
//                              with "submit" send its form
//   click <target> <n>         click element n
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

/** "2.3" as [2, 3]; empty or "0" as [0, 0], the front tab. */
function parseTarget(target) {
	const text = String(target || "").trim();
	if (!text || text === "0" || text === "0.0") return [0, 0];
	const m = /^(\d+)\.(\d+)$/.exec(text);
	if (!m) throw new Error(`bad target "${text}": give window.tab from chrome_tabs, for example 2.3, or leave it out for the front tab`);
	return [Number(m[1]), Number(m[2])];
}

function windowAt(c, number) {
	if (c.windows.length === 0) throw new Error("Chrome has no windows");
	if (number > c.windows.length) throw new Error(`no window ${number}; see chrome_tabs`);
	return c.windows[number > 0 ? number - 1 : 0];
}

function tabAt(c, target) {
	const [wn, tn] = parseTarget(target);
	const w = windowAt(c, wn);
	if (tn > w.tabs.length) throw new Error(`no tab ${wn}.${tn}; see chrome_tabs`);
	return tn > 0 ? w.tabs[tn - 1] : w.activeTab();
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
	try {
		return main(argv);
	} catch (e) {
		return `error: ${e.message}`;
	}
}

function main(argv) {
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
			// "2.3", or "2" "3" as two arguments.
			const target = args.length > 1 ? `${args[0]}.${args[1]}` : args[0];
			if (!String(target || "").trim()) return "error: give the tab as window.tab from chrome_tabs";
			const [wn, tn] = parseTarget(target);
			const w = windowAt(c, wn);
			if (tn < 1 || tn > w.tabs.length) return `error: no tab ${target}; see chrome_tabs`;
			w.activeTabIndex = tn;
			// The window and Chrome come to the front too, so the user sees
			// the tab, on whatever screen or desktop its window is.
			w.index = 1;
			c.activate();
			return `Tab ${target}: ${w.activeTab().title()}, now in front.`;
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
			// The site is open already: say so, and open nothing. The model
			// decides: go to one of those tabs (chrome_tab), or open another
			// one on purpose ("tab"); the user may well want two of a site.
			if (how !== "new" && how !== "tab") {
				const host = (u) => { const m = /^[a-z]+:\/\/(?:www\.)?([^/?#]+)/i.exec(u || ""); return m ? m[1].toLowerCase() : ""; };
				const wanted = host(url);
				const found = [];
				c.windows().forEach((w, i) => w.tabs().forEach((t, j) => {
					if (wanted && host(t.url()) === wanted) found.push(`[${i + 1}.${j + 1}] ${t.title().slice(0, 60)} | ${t.url().slice(0, 80)}`);
				}));
				if (found.length) {
					return `Nothing opened: the site is open already in ${found.length === 1 ? "this tab" : "these tabs"}:\n` +
						found.join("\n") + "\nGo to one with chrome_tab, or call chrome_open again with \"tab\" to open another tab anyway.";
				}
			}
			if (how === "new" || c.windows().length === 0) {
				const w = c.Window().make();
				w.activeTab.url = url;
			} else {
				c.windows[0].tabs.push(c.Tab({ url }));
			}
			c.windows[0].index = 1;
			c.activate();
			return "Opened, now in front. The other chrome_* tools work on it without a target.";
		}
		case "profiles":
			return Object.keys(profiles()).join("\n");
		case "read": {
			const tab = tabAt(c, args[0]);
			const limit = Math.min(Number(args[1]) || 6000, 30000);
			const text = inPage(tab, `document.body ? document.body.innerText.slice(0, ${limit}) : ""`);
			if (text.startsWith("error:")) return text;
			return `${tab.title()}\n${tab.url()}\n\n${text}`;
		}
		case "fields": {
			const tab = tabAt(c, args[0]);
			const answer = inPage(tab, MARK);
			if (answer.startsWith("error:")) return answer;
			const list = JSON.parse(answer || "[]");
			return `${tab.title()}\n` + (list.length ? list.join("\n") : "No fields, buttons or links.");
		}
		case "fill": {
			const [target, n, text, submit] = args;
			const tab = tabAt(c, target);
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
			const tab = tabAt(c, args[0]);
			const n = Number(args[1]);
			const answer = inPage(tab, `(() => {${FIND(n)}
  el.scrollIntoView({ block: "center" }); el.click(); return "ok: " + JSON.stringify(name || el.innerText?.trim().slice(0, 60) || ""); })()`);
			return answer.startsWith("ok") ? `Clicked ${n} ${answer.slice(4)}.` : answer;
		}
		default:
			return "usage: chrome.js tabs | tab <target> | close <w> <t>... | open <url> [profile] [new|tab] | profiles | read <target> [chars] | fields <target> | fill <target> <n> <text> [submit] | click <target> <n>";
	}
}
