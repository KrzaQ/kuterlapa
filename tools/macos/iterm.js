// iTerm2 for Kuterlapa's model tools, through JavaScript for Automation.
// Run: osascript -l JavaScript iterm.js <command> [args]
//
//   tabs                  windows and their tabs: the name of each tab's
//                         session and its folder; * marks the active tab
//   tab <window> <tab>    bring that tab to the front

function folder(session) {
	try {
		return session.variable({ named: "session.path" }) || "";
	} catch (e) {
		return "";
	}
}

function run(argv) {
	const [command, ...args] = argv;
	const iterm = Application("iTerm2");
	switch (command) {
		case "tabs": {
			if (!iterm.running()) return "iTerm is not running.";
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
			const w = iterm.windows[Number(args[0]) - 1];
			w.tabs[Number(args[1]) - 1].select();
			w.select();
			iterm.activate();
			return `Tab ${args[0]}.${args[1]}: ${w.currentTab().currentSession().name()}`;
		}
		default:
			return "usage: iterm.js tabs | tab <window> <tab>";
	}
}
