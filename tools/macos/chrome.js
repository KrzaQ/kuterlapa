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
		default:
			return "usage: chrome.js tabs | tab <window> <tab> | close <window> <tab>... | open <url> [profile] [new] | profiles";
	}
}
