// The migration plan, computed: today's hardcoded strings (the ratchet's own scan, not the committed baseline, which lags main) grouped by area (extract/areas.ts)
// into batches of at most 30 Svelte files or one Rust module group, ordered leaf components first. `i18n/MIGRATION.md`
// is a snapshot of `bun run i18n:extract -- --plan`; run it again after main moves to refresh the counts.
import { join } from "node:path";
import { RUST_DIRS, scanRust } from "../hardcoded/rust.ts";
import { SVELTE_ROOTS, scanSvelte } from "../hardcoded/svelte.ts";
import { cmp, readText, walk } from "../util.ts";
import { areaOf, surfaceOf } from "./areas.ts";

export const MAX_FILES = 30;
/** A Svelte batch stops growing at this many strings: past it one agent's context and one reviewer pass get unreliable. */
export const MAX_STRINGS = 400;

export interface PlanBatch {
	/** `S01`... Svelte, `R01`... Rust. */
	id: string;
	kind: "svelte" | "rust";
	surface: string;
	/** The areas whose files the batch moves: small neighbouring areas share one PR, a big area is split. */
	areas: string[];
	/** `1/2` when an area needs two batches. */
	part?: string;
	files: { file: string; strings: number; fanIn: number }[];
	strings: number;
	/** Mean number of importers of its files: a leaf is 0-1, a shared wrapper is high. Batches go from low to high. */
	fanIn: number;
}

const SRC_FILES = (p: string) => /\.(?:svelte|ts)$/.test(p);

/** How many files import each component, by its file name (`./Foo.svelte`, `#lib/x/Foo.svelte`). */
function fanIn(root: string, files: string[]): Map<string, number> {
	const texts = ["apps/desktop/src", "site/src"].flatMap((d) => walk(join(root, d), SRC_FILES).map(readText));
	const out = new Map<string, number>();
	for (const f of files) {
		const name = f.split("/").pop() as string;
		const needle = [`/${name}"`, `/${name}'`];
		out.set(f, texts.filter((t) => needle.some((n) => t.includes(n))).length);
	}
	return out;
}

export function buildPlan(root: string): PlanBatch[] {
	const scans = { svelte: scanSvelte(root, SVELTE_ROOTS.map((r) => r.dir)).found, rust: scanRust(root, RUST_DIRS) };
	const batches: PlanBatch[] = [];
	for (const kind of ["svelte", "rust"] as const) {
		const counts = Object.fromEntries([...scans[kind]].map(([file, v]) => [file, v.length]));
		const files = Object.keys(counts).sort(cmp);
		const fan = kind === "svelte" ? fanIn(root, files) : new Map<string, number>();
		const byArea = new Map<string, string[]>();
		for (const f of files) {
			const area = areaOf(f) ?? "unmapped";
			byArea.set(area, [...(byArea.get(area) ?? []), f]);
		}
		const made: PlanBatch[] = [];
		for (const [area, list] of byArea) {
			const chunks = Math.ceil(list.length / MAX_FILES);
			for (let i = 0; i < chunks; i++) {
				const part = list.slice(Math.floor((i * list.length) / chunks), Math.floor(((i + 1) * list.length) / chunks));
				const rows = part.map((file) => ({ file, strings: counts[file], fanIn: fan.get(file) ?? 0 }));
				made.push({
					id: "",
					kind,
					surface: surfaceOf(part[0]) ?? "?",
					areas: [area],
					part: chunks > 1 ? `${i + 1}/${chunks}` : undefined,
					files: rows,
					strings: rows.reduce((n, r) => n + r.strings, 0),
					fanIn: rows.reduce((n, r) => n + r.fanIn, 0) / rows.length,
				});
			}
		}
		made.sort((a, b) => (kind === "svelte" ? a.fanIn - b.fanIn : a.strings - b.strings) || cmp(a.areas[0], b.areas[0]) || cmp(a.part ?? "", b.part ?? ""));
		const packed: PlanBatch[] = [];
		for (const b of made) {
			const last = packed[packed.length - 1];
			const fits = last && kind === "svelte" && last.surface === b.surface && !last.part && !b.part && last.files.length + b.files.length <= MAX_FILES && last.strings + b.strings <= MAX_STRINGS;
			if (!fits) {
				packed.push(b);
				continue;
			}
			const n = last.files.length + b.files.length;
			last.fanIn = (last.fanIn * last.files.length + b.fanIn * b.files.length) / n;
			last.areas.push(...b.areas);
			last.files.push(...b.files);
			last.strings += b.strings;
		}
		packed.forEach((b, i) => (b.id = `${kind === "svelte" ? "S" : "R"}${String(i + 1).padStart(2, "0")}`));
		batches.push(...packed);
	}
	return batches;
}

/** The plan as a Markdown table per kind: id, area, files, strings, mean fan-in. */
export function formatPlan(plan: PlanBatch[]): string {
	const out: string[] = [];
	for (const kind of ["svelte", "rust"] as const) {
		const rows = plan.filter((b) => b.kind === kind);
		out.push(`| Batch | Areas | Surface | Files | Strings | Fan-in |`, `|---|---|---|---:|---:|---:|`);
		for (const b of rows) out.push(`| ${b.id} | ${b.areas.map((a) => `\`${a}\``).join(", ")}${b.part ? ` (${b.part})` : ""} | ${b.surface} | ${b.files.length} | ${b.strings} | ${b.fanIn.toFixed(1)} |`);
		out.push(`| | **${kind} total** | | ${rows.reduce((n, b) => n + b.files.length, 0)} | ${rows.reduce((n, b) => n + b.strings, 0)} | |`, "");
	}
	return out.join("\n");
}
