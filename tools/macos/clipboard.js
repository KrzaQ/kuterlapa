// The macOS clipboard for Kuterlapa's model tools, through JavaScript for
// Automation. Run: osascript -l JavaScript clipboard.js <command> [args]
//
//   read [chars]     what the clipboard holds: text, or the names of files,
//                    or what kind of data it is
//   write <text>     put text on the clipboard
//   paste            paste into the application in front (cmd+V), the way
//                    the user would; the model does not see what it pastes
//
// A password manager marks a password it copies as concealed
// (org.nspasteboard.ConcealedType). Then `read` does not show it, but
// `paste` still pastes it.

ObjC.import("AppKit");

function board() {
	return $.NSPasteboard.generalPasteboard;
}

function types() {
	const list = board().types;
	return list.isNil() ? [] : list.js.map((t) => t.js);
}

function concealed() {
	return types().some((t) => t === "org.nspasteboard.ConcealedType" || t === "org.nspasteboard.TransientType");
}

function frontApp() {
	const app = $.NSWorkspace.sharedWorkspace.frontmostApplication;
	return app.isNil() ? "?" : app.localizedName.js;
}

function shell(command) {
	const app = Application.currentApplication();
	app.includeStandardAdditions = true;
	return app.doShellScript(command);
}

function quote(s) {
	return "'" + String(s).replace(/'/g, "'\\''") + "'";
}

/** A key through Kuterlapa's app, like the controller's keys. */
function key(combo) {
	const env = $.NSProcessInfo.processInfo.environment.objectForKey("XDG_STATE_HOME");
	const state = env.isNil() ? $("~/.local/state").stringByExpandingTildeInPath.js : env.js;
	return shell(`printf 'key %s\\n' ${quote(combo)} | nc -U -w 3 ${quote(state + "/kuterlapa/shell.sock")}`).trim();
}

function run(argv) {
	const [command, ...args] = argv;
	switch (command) {
		case "read": {
			if (concealed()) {
				return "The clipboard holds something a password manager marked as secret, so it is not shown. clipboard_paste still pastes it.";
			}
			const limit = Math.min(Number(args[0]) || 6000, 30000);
			const text = board().stringForType($.NSPasteboardTypeString);
			if (!text.isNil() && text.js.length) {
				const t = text.js;
				const more = t.length > limit ? `\n(${t.length - limit} more characters)` : "";
				return `Text, ${t.length} characters:\n${t.slice(0, limit)}${more}`;
			}
			const urls = board().readObjectsForClassesOptions($([$.NSURL]), $({}));
			if (!urls.isNil() && urls.count > 0) {
				return "Files:\n" + urls.js.map((u) => u.path.js || u.absoluteString.js).join("\n");
			}
			const kinds = types();
			return kinds.length ? `No text. The clipboard holds: ${kinds.slice(0, 8).join(", ")}` : "The clipboard is empty.";
		}
		case "write": {
			const text = args.join(" ");
			board().clearContents;
			board().setStringForType($(text), $.NSPasteboardTypeString);
			return `Put ${text.length} characters on the clipboard.`;
		}
		case "paste": {
			if (types().length === 0) return "error: the clipboard is empty";
			const app = frontApp();
			const answer = key("cmd+v");
			return answer === "ok" ? `Pasted into ${app}.` : `error: the paste did not happen: ${answer || "no answer from Kuterlapa"}`;
		}
		default:
			return "usage: clipboard.js read [chars] | write <text> | paste";
	}
}
