// iTerm2 for Kuterlapa's model tools, through JavaScript for Automation.
// Run: osascript -l JavaScript iterm.js <command> [args]
//
//   tabs                          windows and their tabs: the name of each
//                                 tab's session and its folder; * marks the
//                                 active tab
//   tab <target>                  bring that tab, its window and iTerm to
//                                 the front
//   read <target> [lines]         the last lines of a tab's text
//   run <target> <command> [seconds]
//                                 type the command into the tab and answer
//                                 with only the text that came after it
//   new [command]                 a new tab in the front window, and run the
//                                 command in it; iTerm comes to the front
//   close <target>                close a tab (the target is required)
//   interrupt <target>            press Ctrl+C in a tab
//
// A target is "window.tab" from `tabs`, for example "2.3". An empty target
// means the front window's active tab: the one the user sees. Models work
// there unless the user named another tab.
// The text goes to the tab itself, whatever window is in front: typing
// keys would go to the application in front instead.

function folder(session) {
	try {
		return session.variable({ named: "session.path" }) || "";
	} catch (e) {
		return "";
	}
}

/** "2.3" as [2, 3]; empty or "0" as [0, 0], the front tab. */
function parseTarget(target) {
	const text = String(target || "").trim();
	if (!text || text === "0" || text === "0.0") return [0, 0];
	const m = /^(\d+)\.(\d+)$/.exec(text);
	if (!m) throw new Error(`bad target "${text}": give window.tab from iterm_tabs, for example 2.3, or leave it out for the front tab`);
	return [Number(m[1]), Number(m[2])];
}

function windowAt(iterm, number) {
	if (number > 0) {
		if (number > iterm.windows.length) throw new Error(`no window ${number}; see iterm_tabs`);
		return iterm.windows[number - 1];
	}
	return iterm.currentWindow();
}

function tabAt(iterm, target) {
	const [wn, tn] = parseTarget(target);
	const w = windowAt(iterm, wn);
	if (tn > w.tabs.length) throw new Error(`no tab ${wn}.${tn}; see iterm_tabs`);
	return { window: w, tab: tn > 0 ? w.tabs[tn - 1] : w.currentTab() };
}

function sessionAt(iterm, target) {
	return tabAt(iterm, target).tab.currentSession();
}

/** The text of a session without the empty lines at its end. */
function lines(session) {
	const all = session.contents().split("\n");
	while (all.length && !all[all.length - 1].trim()) all.pop();
	return all;
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
	const iterm = Application("iTerm2");
	if (command !== "new" && !iterm.running()) return "iTerm is not running.";
	switch (command) {
		case "tabs": {
			const out = [];
			iterm.windows().forEach((w, i) => {
				let current = "";
				try {
					current = w.currentSession().id();
				} catch (e) {}
				out.push(`Window ${i + 1}`);
				w.tabs().forEach((t, j) => {
					const s = t.currentSession();
					const mark = s.id() === current ? "*" : " ";
					const where = folder(s);
					out.push(`${mark} [${i + 1}.${j + 1}] ${s.name()}${where ? " | " + where : ""}`);
				});
			});
			return out.join("\n") || "No windows.";
		}
		case "tab": {
			// "2.3", or "2" "3" as two arguments.
			const target = args.length > 1 ? `${args[0]}.${args[1]}` : args[0];
			if (!String(target || "").trim()) return "error: give the tab as window.tab from iterm_tabs";
			const { window: w, tab } = tabAt(iterm, target);
			tab.select();
			// The window and iTerm come to the front too, so the user sees
			// the tab, on whatever screen or desktop its window is.
			w.select();
			iterm.activate();
			return `Tab ${target}: ${tab.currentSession().name()}, now in front.`;
		}
		case "read": {
			const s = sessionAt(iterm, args[0]);
			const count = Math.min(Number(args[1]) || 40, 400);
			return `${s.name()}:\n` + lines(s).slice(-count).join("\n");
		}
		case "run": {
			const [target, text, seconds] = args;
			const s = sessionAt(iterm, target);
			const before = lines(s).length;
			s.write({ text });
			// Wait for output: until the text stops changing for a second,
			// or the time is up.
			const limit = Date.now() + Math.min(Number(seconds) || 10, 120) * 1000;
			let last = "", still = 0;
			while (Date.now() < limit) {
				delay(0.3);
				const now = s.contents();
				still = now === last ? still + 0.3 : 0;
				last = now;
				if (still >= 1.0 && lines(s).length > before) break;
			}
			const after = lines(s);
			// The new part: from the last line that shows the command.
			let start = after.length - 1;
			while (start > 0 && !after[start].includes(text.slice(0, 40))) start--;
			const fresh = start > 0 ? after.slice(start) : after.slice(Math.max(before - 1, 0));
			const done = Date.now() < limit ? "" : "\n(still running when the time was up; read the tab again later)";
			return fresh.slice(-200).join("\n") + done;
		}
		case "new": {
			const w = iterm.windows().length ? iterm.currentWindow() : iterm.createWindowWithDefaultProfile();
			const tab = iterm.windows().length && w.tabs().length ? w.createTabWithDefaultProfile() : w.currentTab();
			const text = args.join(" ").trim();
			if (text) tab.currentSession().write({ text });
			// The user asked for the tab: show it.
			w.select();
			iterm.activate();
			return `New tab, now in front${text ? `, ran: ${text}` : ""}. iterm_read without a target reads it.`;
		}
		case "close": {
			// Closing stops what runs in the tab: never by default.
			if (!String(args[0] || "").trim()) return "error: name the tab to close as window.tab from iterm_tabs";
			const { tab } = tabAt(iterm, args[0]);
			const name = tab.currentSession().name();
			tab.close();
			return `Closed ${args[0]} ${name}`;
		}
		case "interrupt": {
			const s = sessionAt(iterm, args[0]);
			s.write({ text: "\u0003", newline: false });
			return `Ctrl+C sent to ${s.name()}`;
		}
		default:
			return "usage: iterm.js tabs | tab <target> | read <target> [lines] | run <target> <command> [seconds] | new [command] | close <target> | interrupt <target>";
	}
}
