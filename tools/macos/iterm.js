// iTerm2 for Kuterlapa's model tools, through JavaScript for Automation.
// Run: osascript -l JavaScript iterm.js <command> [args]
//
//   tabs                          windows and their tabs: the name of each
//                                 tab's session and its folder; * marks the
//                                 active tab
//   tab <window> <tab>            bring that tab to the front
//   read <window> <tab> [lines]   the last lines of a tab's text
//   run <window> <tab> <command> [seconds]
//                                 type the command into the tab and answer
//                                 with only the text that came after it
//   new [command]                 a new tab in the front window, and run the
//                                 command in it
//   close <window> <tab>          close a tab
//   interrupt <window> <tab>      press Ctrl+C in a tab
//
// Window and tab 0 (or nothing) mean the front window and its active tab.
// The text goes to the tab itself, whatever window is in front: typing
// keys would go to the application in front instead.

function folder(session) {
	try {
		return session.variable({ named: "session.path" }) || "";
	} catch (e) {
		return "";
	}
}

function windowAt(iterm, number) {
	const n = Number(number) || 0;
	return n > 0 ? iterm.windows[n - 1] : iterm.currentWindow();
}

function sessionAt(iterm, windowNumber, tabNumber) {
	const w = windowAt(iterm, windowNumber);
	const t = Number(tabNumber) || 0;
	return t > 0 ? w.tabs[t - 1].currentSession() : w.currentSession();
}

/** The text of a session without the empty lines at its end. */
function lines(session) {
	const all = session.contents().split("\n");
	while (all.length && !all[all.length - 1].trim()) all.pop();
	return all;
}

function run(argv) {
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
			const w = windowAt(iterm, args[0]);
			w.tabs[Number(args[1]) - 1].select();
			w.select();
			iterm.activate();
			return `Tab ${args[0]}.${args[1]}: ${w.currentTab().currentSession().name()}`;
		}
		case "read": {
			const s = sessionAt(iterm, args[0], args[1]);
			const count = Math.min(Number(args[2]) || 40, 400);
			return `${s.name()}:\n` + lines(s).slice(-count).join("\n");
		}
		case "run": {
			const [w, t, text, seconds] = args;
			const s = sessionAt(iterm, w, t);
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
			return `New tab${text ? `, ran: ${text}` : ""}. Use iterm_read on the front window's active tab to see it.`;
		}
		case "close": {
			const w = windowAt(iterm, args[0]);
			const tab = w.tabs[Number(args[1]) - 1];
			const name = tab.currentSession().name();
			tab.close();
			return `Closed ${args[0]}.${args[1]} ${name}`;
		}
		case "interrupt": {
			const s = sessionAt(iterm, args[0], args[1]);
			s.write({ text: "\u0003", newline: false });
			return `Ctrl+C sent to ${s.name()}`;
		}
		default:
			return "usage: iterm.js tabs | tab <w> <t> | read <w> <t> [lines] | run <w> <t> <command> [seconds] | new [command] | close <w> <t> | interrupt <w> <t>";
	}
}
