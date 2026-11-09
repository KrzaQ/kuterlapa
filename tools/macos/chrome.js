// Google Chrome for Kuterlapa's model tools, through JavaScript for
// Automation. Run: osascript -l JavaScript chrome.js <command> [args]
//
//   tabs                       windows and their tabs; * marks the active tab
//   tab <window> <tab>         bring that tab to the front
//   close <window> <tab>...    close tabs of one window (cmd+shift+t in
//                              Chrome brings a tab back)
//   open <url> [profile] [new] open the address in a new tab of the front
//                              window; with a profile name, in a window of
//                              that profile; "new": in a new window
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
const MARK = `(() => {
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
			const code = `(() => {
  const el = document.querySelector('[data-kuterlapa="${Number(n)}"]');
  if (!el) return "error: no element ${Number(n)}; list the fields first";
  if (el.type === "password") return "error: Kuterlapa does not fill passwords";
  el.focus();
  const text = ${JSON.stringify(text || "")};
  if (el.isContentEditable) { document.execCommand("selectAll"); document.execCommand("insertText", false, text); }
  else {
    // The native setter, so frameworks such as React notice the change.
    const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : el.tagName === "SELECT" ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }
  if (${submit === "submit"}) {
    if (el.form) { el.form.requestSubmit ? el.form.requestSubmit() : el.form.submit(); }
    else { for (const type of ["keydown", "keypress", "keyup"]) el.dispatchEvent(new KeyboardEvent(type, { key: "Enter", code: "Enter", keyCode: 13, bubbles: true })); }
  }
  return "ok";
})()`;
			const answer = inPage(tab, code);
			return answer === "ok" ? `Filled ${n}${submit === "submit" ? " and sent the form" : ""}.` : answer;
		}
		case "click": {
			const tab = tabAt(c, args[0], args[1]);
			const n = Number(args[2]);
			const answer = inPage(tab, `(() => { const el = document.querySelector('[data-kuterlapa="${n}"]');
  if (!el) return "error: no element ${n}; list the fields first"; el.scrollIntoView({ block: "center" }); el.click(); return "ok"; })()`);
			return answer === "ok" ? `Clicked ${n}.` : answer;
		}
		default:
			return "usage: chrome.js tabs | tab <w> <t> | close <w> <t>... | open <url> [profile] [new] | profiles | read <w> <t> [chars] | fields <w> <t> | fill <w> <t> <n> <text> [submit] | click <w> <t> <n>";
	}
}
