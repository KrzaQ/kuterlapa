// Gives pi the Kuterlapa tools for one run, without touching pi's own
// settings. The core writes this file to its state folder and passes it
// with `pi -e <file>`, together with two environment variables:
//
//   KUTERLAPA_EXE      the kuterlapa program, which serves the tools over MCP
//   KUTERLAPA_SESSION  the session whose tools the model may use
//   KUTERLAPA_LOCAL    "1" when the model runs locally: private tools too
//
// The tools are "direct": declared to the model like pi's own tools.

export default function (pi: any) {
	const exe = process.env.KUTERLAPA_EXE;
	if (!exe) return;
	const args = ["mcp"];
	const session = process.env.KUTERLAPA_SESSION;
	if (session) args.push("--session", session);
	if (process.env.KUTERLAPA_LOCAL === "1") args.push("--local");
	pi.registerMcpServer("kuterlapa", {
		command: exe,
		args,
		exposure: "direct",
		description: "Kuterlapa tools: the user's devices and programs",
	});
}
