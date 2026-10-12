// `bun studio capture <app|tui|list> …`: the dispatcher calls `main(args)` with everything after `capture`.

import { parseArgs } from "node:util";
import { hasAsset } from "../lib/assets.ts";
import { StudioError } from "../lib/paths.ts";
import { captureScene } from "./app.ts";
import type { Scene } from "./ctx.ts";
import agents from "./scenes/agents.ts";
import logs from "./scenes/logs.ts";
import mcp from "./scenes/mcp.ts";
import panel from "./scenes/panel.ts";
import spaces from "./scenes/spaces.ts";
import terminal from "./scenes/terminal.ts";
import { captureTui } from "./tui.ts";

const BY_ID: Record<string, Scene> = { spaces, terminal, panel, logs, agents, mcp };
const SCENES = Object.keys(BY_ID);
const TUI_TAKES = ["tui"];

const HELP = `bun studio capture <command>

  app <scene|--all> [--as <key>]   record an app scene from an isolated Atlas Desktop (debug bundle) -> rec:<scene>
                                   scenes: ${SCENES.join(", ")}
                                   --as   register under <key> instead of rec:<scene> (e.g. rec:panel@new)
  tui <take> [--tape <path>]       run a vhs tape (default demo/demo.tape) against target/debug/atlas -> take:<take>
  list                             scenes and takes, and whether each is in the manifest

Needs the debug bundle and CLI (cargo build -p atlas-cli; the desktop debug bundle), Screen Recording permission for
the terminal running this, swiftc (recorder), and vhs for tui. Nothing is sent to your desktop; the instance runs with
its own HOME and hostd dir.`;

export async function main(args: string[]): Promise<void> {
	const [cmd, ...rest] = args;
	if (!cmd || cmd === "--help" || cmd === "-h" || cmd === "help") {
		console.log(HELP);
		return;
	}
	const { values, positionals } = parseArgs({
		args: rest,
		allowPositionals: true,
		options: { all: { type: "boolean" }, as: { type: "string" }, tape: { type: "string" }, help: { type: "boolean", short: "h" } },
	});
	if (values.help) {
		console.log(HELP);
		return;
	}
	switch (cmd) {
		case "list":
			for (const id of SCENES) console.log(`app  ${id.padEnd(9)} rec:${id.padEnd(9)} ${hasAsset(`rec:${id}`) ? "in manifest" : "missing"}`);
			for (const t of TUI_TAKES) console.log(`tui  ${t.padEnd(9)} take:${t.padEnd(8)} ${hasAsset(`take:${t}`) ? "in manifest" : "missing"}`);
			return;
		case "app": {
			const ids = values.all ? SCENES : positionals.slice(0, 1);
			if (!ids.length) throw new StudioError("which scene?", `bun studio capture app <${SCENES.join("|")}|--all>`);
			if (values.as && ids.length > 1) throw new StudioError("--as names one recording", "use it with a single scene");
			for (const id of ids) {
				const scene = BY_ID[id];
				if (!scene) throw new StudioError(`unknown scene "${id}"`, `scenes: ${SCENES.join(", ")}`);
				console.log(`▶ ${id}`);
				const key = await captureScene(scene, { as: values.as });
				console.log(`✓ ${key}`);
			}
			return;
		}
		case "tui": {
			const take = positionals[0];
			if (!take) throw new StudioError("which take?", "bun studio capture tui <take> [--tape <path>]   (the film uses: tui)");
			console.log(`✓ ${await captureTui(take, { tape: values.tape })}`);
			return;
		}
		default:
			throw new StudioError(`unknown capture command "${cmd}"`, HELP);
	}
}
