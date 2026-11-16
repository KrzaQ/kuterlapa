// Google Chrome for Kuterlapa's model tools, through JavaScript for
// Automation. Run: osascript -l JavaScript chrome.js <command> [args]
//
//   tabs                       windows and their tabs; * marks the active tab
//   tab <target>               bring that tab, its window and Chrome to the
//                              front
//   close [targets] [yes]      close the tab the user sees, or the given
//                              tabs ("1.5 1.7"); tabs other than the one in
//                              front need "yes" (cmd+shift+t brings a tab
//                              back)
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
//   actions <target>           the site pack's actions for this page
//   do <target> <action> [json]
//                              run one of them, with its arguments as JSON
//
// computer_windows shows which profile each Chrome window belongs to.

ObjC.import("Foundation");
ObjC.import("AppKit");

/** The application in front: its name, and whether it is Chrome. */
function front() {
	const app = $.NSWorkspace.sharedWorkspace.frontmostApplication;
	if (app.isNil()) return { name: "?", chrome: false };
	return { name: app.localizedName.js, chrome: app.bundleIdentifier.js === "com.google.Chrome" };
}

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

// Site packs: actions for one site, loaded by the address of the tab, so
// that the model's tool list stays the same for every site. A pack is a
// folder with site.json and the page scripts it names:
//
//   sites/<pack>/site.json   built in, next to this script
//   ~/.config/kuterlapa/sites/<pack>/site.json   the user's own; it
//                            replaces a built-in pack of the same folder name
//
//   {
//     "name": "WhatsApp",
//     "match": ["web.whatsapp.com"],      host, or host/path prefix
//     "hint": "One or two sentences for the model.",
//     "include": ["../lib/names.js"],     scripts put before each action's
//     "actions": {
//       "open_chat": {
//         "description": "...",
//         "params": { "name": "..." },    all required, all strings
//         "script": "chats.js",           runs in the page
//         "check": "check.js",            optional: runs `wait` seconds later
//         "wait": 1
//       }
//     }
//   }
//
// A page script gets `action`, `args` and, in a check or a second round,
// `result` (what the script returned). It returns text for the model, or an
// object: its "text" is the answer, and with a check the whole object goes
// to it. {"again": true, "wait": 1} runs the script again a second later
// (at most 4 times); {"click": {"x", "y"}} clicks there with the real mouse
// (screen points) on the tab the user sees.

function readFile(path) {
	const text = $.NSString.stringWithContentsOfFileEncodingError($(path).stringByStandardizingPath, $.NSUTF8StringEncoding, null);
	return text.isNil() ? null : text.js;
}

function scriptDir() {
	const args = $.NSProcessInfo.processInfo.arguments.js.map((a) => a.js);
	const me = args.find((a) => /chrome\.js$/.test(a)) || "./chrome.js";
	const full = me.startsWith("/") ? me : `${$.NSFileManager.defaultManager.currentDirectoryPath.js}/${me}`;
	return $(full).stringByDeletingLastPathComponent.js;
}

function packs() {
	const config = $.NSProcessInfo.processInfo.environment.objectForKey("XDG_CONFIG_HOME");
	const configDir = config.isNil() ? $("~/.config").stringByExpandingTildeInPath.js : config.js;
	const byFolder = {};
	for (const root of [`${scriptDir()}/sites`, `${configDir}/kuterlapa/sites`]) {
		const names = $.NSFileManager.defaultManager.contentsOfDirectoryAtPathError(root, null);
		if (names.isNil()) continue;
		for (const folder of names.js.map((n) => n.js)) {
			const text = readFile(`${root}/${folder}/site.json`);
			if (!text) continue;
			try {
				byFolder[folder] = { ...JSON.parse(text), dir: `${root}/${folder}` };
			} catch (e) {
				byFolder[folder] = { name: folder, match: [], broken: `${root}/${folder}/site.json: ${e.message}` };
			}
		}
	}
	return Object.values(byFolder);
}

function packFor(url) {
	const m = /^[a-z]+:\/\/([^/?#]+)([^?#]*)/i.exec(url || "");
	if (!m) return null;
	const host = m[1].toLowerCase(), path = m[2] || "/";
	return packs().find((p) => (p.match || []).some((entry) => {
		const [h, ...rest] = entry.toLowerCase().split("/");
		const prefix = rest.length ? "/" + rest.join("/") : "";
		return (host === h || host.endsWith("." + h)) && path.startsWith(prefix);
	})) || null;
}

/** One line for listings: which actions the site has. */
function siteNote(url) {
	const pack = packFor(url);
	if (!pack || !pack.actions) return "";
	return `Site actions (${pack.name}): ${Object.keys(pack.actions).join(", ")}. See chrome_actions; prefer them to clicking and searching.`;
}

function pageScript(pack, file, action, args, result) {
	const parts = [...(pack.include || []), file].map((f) => {
		const text = readFile(`${pack.dir}/${f}`);
		if (text === null) throw new Error(`site pack ${pack.name}: no file ${f}`);
		return text;
	});
	const body = parts.pop();
	return `(() => {
const action = ${JSON.stringify(action)};
const args = ${JSON.stringify(args)};
const result = ${JSON.stringify(result ?? null)};
${parts.join("\n")}
const out = (() => {
${body}
})();
return typeof out === "string" ? out : JSON.stringify(out);
})()`;
}

/** Sends a line to Kuterlapa's app (shell.sock) and returns its answer. */
function app(line) {
	const env = $.NSProcessInfo.processInfo.environment.objectForKey("XDG_STATE_HOME");
	const state = env.isNil() ? $("~/.local/state").stringByExpandingTildeInPath.js : env.js;
	return shell(`printf '%s\\n' ${quote(line)} | nc -U -w 3 ${quote(state + "/kuterlapa/shell.sock")}`).trim();
}

/** A real key, through Kuterlapa's app, on the tab the user sees. */
function realKey(c, tab, combo) {
	const w = c.windows[0];
	if (w.activeTab().id() !== tab.id() || !front().chrome) return "error: not the tab in front";
	const answer = app(`key ${combo}`);
	return answer === "ok" ? "ok" : `error: ${answer || "no answer from Kuterlapa"}`;
}

/** A click of the real mouse at a screen point, through Kuterlapa's app:
 * some pages (WhatsApp) ignore mouse events made in the page. Only on the
 * tab the user sees, which comes to the front first. */
function realClick(c, tab, point) {
	const w = c.windows[0];
	if (w.activeTab().id() !== tab.id()) return "error: this action clicks, so it works only on the tab the user sees; switch to it with chrome_tab";
	w.index = 1;
	c.activate();
	delay(0.3);
	const answer = app(`click-points ${Math.round(point.x)} ${Math.round(point.y)}`);
	return answer === "ok" ? "ok" : `error: the click did not happen: ${answer || "no answer from Kuterlapa"}`;
}

function answerOf(text) {
	if (text.startsWith("{")) {
		try {
			return JSON.parse(text);
		} catch (e) {}
	}
	return { text };
}

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
			if (c.windows().length) {
				const w = c.windows[0];
				const tab = `[1.${w.activeTabIndex()}] ${w.activeTab().title().slice(0, 80)}`;
				const app = front();
				// Chrome's front window is not what the user sees when another
				// application, or another desktop, is in front.
				out.push(app.chrome
					? `The user sees: ${tab}`
					: `Chrome is not in front: the user sees ${app.name}. Chrome's front window shows ${tab}; chrome_tab brings it to the user.`);
				const note = siteNote(w.activeTab().url());
				if (note) out.push(note);
			}
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
			const note = siteNote(w.activeTab().url());
			return `Tab ${target}: ${w.activeTab().title()}, now in front.${note ? "\n" + note : ""}`;
		}
		case "close": {
			// No tabs: the tab the user sees. Others only with "yes": a model
			// once closed the tab it had worked in, while the user meant the
			// new one in front.
			const [list, sure] = args;
			const app = front();
			if (!String(list || "").trim() && !app.chrome) {
				return `error: Chrome is not in front: the user sees ${app.name}, so "this tab" is probably not a Chrome tab. Give Chrome tabs by number only when the user meant Chrome.`;
			}
			const w1 = windowAt(c, 0);
			const seen = [1, w1.activeTabIndex()];
			const wanted = String(list || "").trim()
				? String(list).trim().split(/[\s,]+/).map(parseTarget)
				: [seen];
			const others = wanted.filter(([w, t]) => !(w === 1 && t === seen[1]) && !(w === 0 && t === 0));
			if (others.length && sure !== "yes") {
				const names = others.map(([w, t]) => {
					const win = windowAt(c, w);
					return `${w}.${t} ${t >= 1 && t <= win.tabs.length ? win.tabs[t - 1].title() : "(no such tab)"}`;
				});
				return `error: not the tab the user sees (${seen.join(".")} ${w1.activeTab().title()}): ${names.join("; ")}. ` +
					"\"This tab\" means the one the user sees: call chrome_close without tabs. To close the others, ask the user first (ask_user), then call again with confirm = \"yes\".";
			}
			// Highest numbers first, so the numbers still to close stay right.
			const resolved = wanted.map(([w, t]) => (w === 0 && t === 0 ? seen : [w, t]))
				.sort((a, b) => b[0] - a[0] || b[1] - a[1]);
			const closed = [];
			for (const [w, t] of resolved) {
				const win = windowAt(c, w);
				if (t < 1 || t > win.tabs.length) { closed.push(`${w}.${t}: no such tab`); continue; }
				const tab = win.tabs[t - 1];
				closed.push(`${w}.${t} ${tab.title()}`);
				tab.close();
			}
			return `Closed:\n${closed.join("\n")}`;
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
			const note = siteNote(tab.url());
			return `${tab.title()}\n${tab.url()}${note ? "\n" + note : ""}\n\n${text}`;
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
  return "ok";
})()`;
			const answer = inPage(tab, code);
			if (answer !== "ok") return answer;
			if (submit === "submit") {
				// A real Enter, as the user would press it: a page such as
				// Gmail handles that itself, but a form sent from a script
				// reloads the whole page. Only on the tab the user sees.
				const sent = realKey(c, tab, "Return");
				if (sent !== "ok") {
					inPage(tab, `(() => { const el = document.activeElement; for (const type of ["keydown", "keypress", "keyup"]) el && el.dispatchEvent(new KeyboardEvent(type, { key: "Enter", code: "Enter", keyCode: 13, bubbles: true })); return "ok"; })()`);
					return `Filled ${n} and pressed Enter in the page (not the tab in front, so no real key).`;
				}
				return `Filled ${n} and pressed Enter.`;
			}
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
		case "actions": {
			const tab = tabAt(c, args[0]);
			const url = tab.url();
			const pack = packFor(url);
			if (!pack) return `No site pack for ${url.slice(0, 80)}. Use chrome_read, chrome_fields, chrome_fill and chrome_click.`;
			if (pack.broken) return `error: ${pack.broken}`;
			const out = [`${pack.name}${pack.hint ? ": " + pack.hint : ""}`, "Actions, for chrome_do:"];
			for (const [name, a] of Object.entries(pack.actions || {})) {
				const params = Object.keys(a.params || {});
				out.push(`- ${name}(${params.join(", ")}): ${a.description || ""}`);
				for (const [p, d] of Object.entries(a.params || {})) out.push(`    ${p}: ${d}`);
			}
			return out.join("\n");
		}
		case "do": {
			const [target, name, json] = args;
			const tab = tabAt(c, target);
			const pack = packFor(tab.url());
			if (!pack) return "error: no site pack for this page; see chrome_actions";
			if (pack.broken) return `error: ${pack.broken}`;
			const action = (pack.actions || {})[name];
			if (!action) return `error: ${pack.name} has no action "${name}"; it has ${Object.keys(pack.actions || {}).join(", ")}`;
			let given = {};
			if (String(json || "").trim()) {
				try {
					given = JSON.parse(json);
				} catch (e) {
					return `error: args must be a JSON object, for example {"name": "Anna"}: ${e.message}`;
				}
			}
			const missing = Object.keys(action.params || {}).filter((p) => given[p] === undefined || given[p] === "");
			if (missing.length) return `error: ${name} needs ${missing.join(", ")}`;
			let first = inPage(tab, pageScript(pack, action.script, name, given));
			if (first.startsWith("error:")) return first;
			let answer = answerOf(first);
			// A script may ask to run again a moment later, with its answer
			// as `result`: after it typed a search, to read the results.
			for (let round = 0; answer.again && round < 4; round++) {
				delay(Number(answer.wait) || 1);
				first = inPage(tab, pageScript(pack, action.script, name, given, answer));
				if (first.startsWith("error:")) return first;
				answer = answerOf(first);
			}
			if (answer.error) return `error: ${answer.error}`;
			if (answer.click) {
				const done = realClick(c, tab, answer.click);
				if (done !== "ok") return done;
			}
			if (!action.check) return answer.text ?? first;
			delay(Number(action.wait) || 1);
			const checked = inPage(tab, pageScript(pack, action.check, name, given, answer));
			if (checked.startsWith("error:")) return checked;
			const final = answerOf(checked);
			return final.error ? `error: ${final.error}` : (final.text ?? checked);
		}
		case "click": {
			const tab = tabAt(c, args[0]);
			const n = Number(args[1]);
			const answer = inPage(tab, `(() => {${FIND(n)}
  el.scrollIntoView({ block: "center" }); el.click(); return "ok: " + JSON.stringify(name || el.innerText?.trim().slice(0, 60) || ""); })()`);
			return answer.startsWith("ok") ? `Clicked ${n} ${answer.slice(4)}.` : answer;
		}
		default:
			return "usage: chrome.js tabs | tab <target> | close [targets] [yes] | open <url> [profile] [new|tab] | profiles | read <target> [chars] | fields <target> | fill <target> <n> <text> [submit] | click <target> <n> | actions <target> | do <target> <action> [json]";
	}
}
