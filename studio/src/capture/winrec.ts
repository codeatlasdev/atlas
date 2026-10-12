// Compile-on-demand of the ScreenCaptureKit window recorder: studio/native/winrec.swift -> studio/.cache/bin/winrec-<hash>.
// The source hash is part of the file name, so editing the Swift file rebuilds and an unchanged one is reused.

import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { run, sha256 } from "../lib/exec.ts";
import { CACHE, STUDIO, StudioError } from "../lib/paths.ts";

const SOURCE = join(STUDIO, "native/winrec.swift");
const BIN_DIR = join(CACHE, "bin");

/** Absolute path of a built `winrec`, compiling it with `swiftc -O` first if the source changed or it is missing. */
export async function ensureWinrec(): Promise<string> {
	if (process.platform !== "darwin") throw new StudioError("window capture needs macOS (ScreenCaptureKit)", "run the capture on a Mac");
	const hash = sha256(await Bun.file(SOURCE).text()).slice(0, 12);
	const bin = join(BIN_DIR, `winrec-${hash}`);
	if (existsSync(bin)) return bin;
	mkdirSync(BIN_DIR, { recursive: true });
	// only our own compiled recorders (winrec-<hash>, plain files) inside .cache/bin
	for (const old of readdirSync(BIN_DIR)) if (/^winrec-[0-9a-f]{12}$/.test(old)) rmSync(join(BIN_DIR, old), { force: true });
	console.log(`compiling winrec (${hash}) …`);
	await run(["swiftc", "-O", SOURCE, "-o", bin]);
	return bin;
}
