// Content-addressed asset cache. Every heavy input (window recordings, vhs takes, backgrounds, voice lines, music)
// lives under studio/.cache (gitignored) and is listed in studio/.cache/manifest.json with its sha256 and where it
// came from, so a re-render reuses takes and `bun studio assets verify` can tell a stale or missing file from a good one.

import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative } from "node:path";
import { sha256File } from "./exec.ts";
import { CACHE, STUDIO, StudioError } from "./paths.ts";

export type AssetKind = "recording" | "take" | "background" | "voice" | "music" | "proxy" | "image";

export interface AssetEntry {
	/** stable key used by film data: `rec:spaces`, `take:tui`, `bg:a`, `music:bed`, `voice:<hash>` */
	key: string;
	kind: AssetKind;
	/** path relative to studio/ */
	path: string;
	sha256: string;
	bytes: number;
	/** how it was produced, e.g. "bun studio capture app spaces" or "imported:legacy" */
	source: string;
	/** free-form provenance the tool wants to keep (prompt, seed, voice settings, git sha of the app build…) */
	meta?: Record<string, unknown>;
}

interface Manifest {
	version: 1;
	assets: Record<string, AssetEntry>;
}

const MANIFEST = join(CACHE, "manifest.json");

function load(): Manifest {
	if (!existsSync(MANIFEST)) return { version: 1, assets: {} };
	return JSON.parse(readFileSync(MANIFEST, "utf8")) as Manifest;
}

function save(m: Manifest): void {
	mkdirSync(dirname(MANIFEST), { recursive: true });
	// sorted keys: the manifest diffs cleanly and is byte-stable for equal content
	const sorted: Manifest = { version: 1, assets: Object.fromEntries(Object.entries(m.assets).sort(([a], [b]) => a.localeCompare(b))) };
	writeFileSync(MANIFEST, `${JSON.stringify(sorted, null, "\t")}\n`);
}

const abs = (p: string) => (isAbsolute(p) ? p : join(STUDIO, p));

/** Register (or replace) an asset. `file` may be anywhere; it is copied under .cache/<kind>/ unless already inside .cache. */
export async function registerAsset(key: string, kind: AssetKind, file: string, source: string, meta?: Record<string, unknown>): Promise<AssetEntry> {
	let dest = abs(file);
	if (!dest.startsWith(`${CACHE}/`)) {
		dest = join(CACHE, kind, `${key.replace(/[^\w.-]+/g, "_")}${dest.slice(dest.lastIndexOf("."))}`);
		mkdirSync(dirname(dest), { recursive: true });
		copyFileSync(abs(file), dest);
	}
	const entry: AssetEntry = { key, kind, path: relative(STUDIO, dest), sha256: await sha256File(dest), bytes: statSync(dest).size, source, ...(meta ? { meta } : {}) };
	const m = load();
	m.assets[key] = entry;
	save(m);
	return entry;
}

/** Absolute path of a registered asset; throws with the command that produces it when it is missing. */
export function assetPath(key: string): string {
	const e = load().assets[key];
	if (!e) throw new StudioError(`asset "${key}" is not in the manifest`, hintFor(key));
	const p = abs(e.path);
	if (!existsSync(p)) throw new StudioError(`asset "${key}" is in the manifest but ${e.path} is gone`, hintFor(key));
	return p;
}

export const hasAsset = (key: string) => key in load().assets && existsSync(abs(load().assets[key]!.path));
export const listAssets = () => Object.values(load().assets);

/** Re-hash every registered file and report mismatches. */
export async function verifyAssets(): Promise<Array<{ key: string; problem: string }>> {
	const bad: Array<{ key: string; problem: string }> = [];
	for (const e of listAssets()) {
		const p = abs(e.path);
		if (!existsSync(p)) bad.push({ key: e.key, problem: "missing file" });
		else if ((await sha256File(p)) !== e.sha256) bad.push({ key: e.key, problem: "sha256 changed" });
	}
	return bad;
}

function hintFor(key: string): string {
	const [kind, name] = key.split(":");
	switch (kind) {
		case "rec":
			return `capture it: bun studio capture app ${name}   (or import a legacy recording: bun studio assets import rec ${name} <file.mov>)`;
		case "take":
			return `capture it: bun studio capture tui ${name}`;
		case "bg":
			return `generate it: bun studio bg ${name}   (needs MAGNIFIC_API_KEY)`;
		case "music":
			return `see: bun studio music --help`;
		default:
			return "run: bun studio assets";
	}
}
