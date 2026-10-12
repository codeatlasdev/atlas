// `capture tui <take>`: run a vhs tape against the debug `atlas` and register the mp4 as `take:<take>`.
// The tape's own `Output` lines are replaced in a temp copy, so the site's demo.gif and demo/.out are never touched.

import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { isAbsolute, join } from "node:path";
import { registerAsset } from "../lib/assets.ts";
import { probe, run } from "../lib/exec.ts";
import { REPO, StudioError } from "../lib/paths.ts";

const ATLAS = join(REPO, "target/debug/atlas");
/** The demo stack (demo/atlas.yaml) listens here; vhs would collide with whatever already holds them. */
const PORTS = [4100, 4101];

export interface TuiCaptureOpts {
	/** Tape to run; default demo/demo.tape. Relative paths resolve from the repo root. */
	tape?: string;
}

export async function captureTui(take: string, opts: TuiCaptureOpts = {}): Promise<string> {
	if (!/^[\w.-]+$/.test(take)) throw new StudioError(`bad take name "${take}"`, "use letters, digits, - _ .");
	const tape = isAbsolute(opts.tape ?? "") ? opts.tape! : join(REPO, opts.tape ?? "demo/demo.tape");
	if (!existsSync(tape)) throw new StudioError(`tape not found: ${tape}`, "pass --tape <path>");
	if (!existsSync(ATLAS)) throw new StudioError(`the atlas CLI is missing: ${ATLAS}`, "build it from the repo root:\n  cargo build -p atlas-cli && codesign -f -s - target/debug/atlas");
	const vhs = await run(["which", "vhs"], { allowFail: true });
	if (vhs.code !== 0) throw new StudioError("vhs is not installed", "brew install vhs   (it also needs ttyd and ffmpeg)");
	for (const port of PORTS) {
		// read-only check of who listens; we never kill anything
		const { out } = await run(["lsof", "-nP", `-iTCP:${port}`, "-sTCP:LISTEN"], { allowFail: true });
		if (out.trim()) throw new StudioError(`port ${port} is in use, the demo stack needs 4100 and 4101`, `stop what listens there (your own dev stack?) and retry:\n${out.trim()}`);
	}

	const tmp = mkdtempSync(join(tmpdir(), "atlas-studio-vhs-"));
	try {
		const out = join(tmp, `${take}.mp4`);
		const text = readFileSync(tape, "utf8")
			.split("\n")
			.filter((l) => !/^\s*Output\s/.test(l))
			.join("\n");
		const temp = join(tmp, "tape.tape");
		writeFileSync(temp, `Output "${out}"\n${text}`);
		// the tape cd's into demo/ and runs ../target/debug/atlas: run from the repo root like its header says
		await run(["vhs", temp], { cwd: REPO, live: true });
		if (!existsSync(out)) throw new StudioError("vhs finished without producing the mp4", "run it by hand: vhs demo/demo.tape");
		const info = await probe(out);
		const { out: ver } = await run([ATLAS, "version"]);
		const { out: sha } = await run(["git", "rev-parse", "--short", "HEAD"], { cwd: REPO });
		const key = `take:${take}`;
		await registerAsset(key, "take", out, `bun studio capture tui ${take}`, {
			atlasVersion: /atlas (\S+)/.exec(ver)?.[1],
			gitSha: sha.trim(),
			tape: tape.startsWith(`${REPO}/`) ? tape.slice(REPO.length + 1) : tape,
			seconds: Math.round(info.duration * 10) / 10,
			size: `${info.width}x${info.height}`,
		});
		return key;
	} finally {
		// never remove anything but our own mkdtemp dir
		if (!tmp.startsWith(`${tmpdir()}/atlas-studio-vhs-`) || tmp.includes("..")) throw new StudioError(`refusing to remove ${tmp}`, "not a scratch dir of this capture");
		rmSync(tmp, { recursive: true, force: true });
		// the tape leaves its .logs under demo/ (it rm -rf's them on the next run); stop only what that stack started
		await run([ATLAS, "down", "--dir", join(REPO, "demo")], { allowFail: true });
	}
}
