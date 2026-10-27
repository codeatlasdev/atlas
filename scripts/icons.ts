// The Atlas brand has ONE source: assets/logo.svg (the mark: 36 hand-cut tiles + the cobalt gradient).
// Everything else (app icon, menu-bar template, animated logo, favicons, touch icon, the Tauri icon set,
// the studio copy, the welcome-intro tile data) is derived from it here and nowhere else. Nothing derived is
// ever edited by hand.
//
//   bun run icons                 regenerate every derived asset (needs rsvg-convert, magick, cargo-tauri)
//   bun run icons -- --check      fail (exit 1) when a derived asset is out of date with its sources
//   bun run icons -- --check --text   same, text outputs + the source lock only (no external tools; what CI runs)
//
// Inputs that are designed, not derived: assets/logo.svg and assets/templates/*.svg (the plate, filters and
// stagger around the tiles; `{{TILES}}` / `{{TILE:i}}` are replaced with the mark's paths).
// assets/icons.lock.json records the sha256 of all inputs and of every derived raster; a raster is stale when
// the inputs changed after it was rendered, or when it was edited by hand. Rasters are re-rendered and
// compared by decoded pixels in the full check (`cargo tauri icon` orders ICNS chunks differently per run).
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

export const ROOT = join(import.meta.dir, "..");
const abs = (p: string) => join(ROOT, p);
const sha = (b: Buffer | string) => createHash("sha256").update(b).digest("hex");

/** The designed inputs. Changing any of them makes every raster below stale until `bun run icons` runs. */
export const SOURCES = ["assets/logo.svg", "assets/templates/app-icon.svg", "assets/templates/tray-template.svg", "assets/templates/logo-animated.svg"];
export const LOCK = "assets/icons.lock.json";

/** Rasters: derived binaries. `pixels` = compared decoded (PNG/ICO/ICNS), never by raw bytes. */
export const RASTERS = [
	"assets/app-icon-1024.png",
	"crates/atlas-desktop/icons/32x32.png",
	"crates/atlas-desktop/icons/128x128.png",
	"crates/atlas-desktop/icons/128x128@2x.png",
	"crates/atlas-desktop/icons/icon.png",
	"crates/atlas-desktop/icons/icon.ico",
	"crates/atlas-desktop/icons/icon.icns",
	"crates/atlas-desktop/icons/tray.png",
	"crates/atlas-desktop/icons/tray-linux.png",
	"site/public/apple-touch-icon.png",
	"site/public/favicon.ico",
];

const read = (p: string) => readFileSync(abs(p), "utf8");

/** Every text output, as path -> content. Pure: only reads the designed inputs. */
export function textOutputs(): Map<string, string> {
	const logo = read("assets/logo.svg");
	const tiles = [...logo.matchAll(/<path d="([^"]+)"/g)].map((m) => m[1]!);
	if (tiles.length === 0) throw new Error("assets/logo.svg has no <path d=...> tiles");
	const out = new Map<string, string>();

	// Tile lines take the indent of the marker they replace, so the output stays readable.
	const fill = (tpl: string) =>
		tpl
			.replace(/^(\s*)\{\{TILES\}\}$/gm, (_, ind: string) => tiles.map((d) => `${ind}<path d="${d}"/>`).join("\n"))
			.replace(/\{\{TILE:(\d+)\}\}/g, (_, i: string) => {
				const d = tiles[Number(i)];
				if (d === undefined) throw new Error(`template asks for tile ${i}, the mark has ${tiles.length}`);
				return d;
			});
	for (const [name, tpl] of [
		["assets/app-icon.svg", "assets/templates/app-icon.svg"],
		["assets/tray-template.svg", "assets/templates/tray-template.svg"],
		["assets/logo-animated.svg", "assets/templates/logo-animated.svg"],
	] as const) {
		const text = fill(read(tpl));
		if (/\{\{/.test(text)) throw new Error(`${tpl}: unreplaced marker`);
		out.set(name, text);
	}

	// Verbatim copies of the mark, for consumers that cannot reach ../assets (the site is deployed on its own,
	// the studio bundles its asset dir).
	out.set("site/public/favicon.svg", logo);
	out.set("studio/assets/logo/logo.svg", logo);

	// The welcome intro animates the real tiles: path data and bounding box per tile.
	out.set("apps/desktop/src/lib/onboarding/intro/mark.ts", markTs(logo, tiles));
	return out;
}

/** Bounding box of an absolute-coordinate path (the traced logo uses only M/L/C/Z with absolute numbers). */
function box(d: string) {
	const n = (d.match(/-?\d+\.?\d*/g) ?? []).map(Number);
	const xs = n.filter((_, i) => i % 2 === 0);
	const ys = n.filter((_, i) => i % 2 === 1);
	const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
	return { cx: +((x0 + x1) / 2).toFixed(1), cy: +((y0 + y1) / 2).toFixed(1), w: +(x1 - x0).toFixed(1), h: +(y1 - y0).toFixed(1) };
}

function markTs(logo: string, tiles: string[]) {
	const view = /viewBox="0 0 (\d+) (\d+)"/.exec(logo);
	if (!view) throw new Error("logo.svg has no 0 0 W H viewBox");
	// Written the way prettier formats it (tab-indented, unquoted keys, trailing commas), so `prettier --check src` stays green.
	const rows = tiles
		.map((d, id) => {
			const b = box(d);
			return `\t{\n\t\tid: ${id},\n\t\tcx: ${b.cx},\n\t\tcy: ${b.cy},\n\t\tw: ${b.w},\n\t\th: ${b.h},\n\t\td: ${JSON.stringify(d)},\n\t},`;
		})
		.join("\n");
	const axis = /<linearGradient[^>]*x1="([^"]+)" y1="([^"]+)" x2="([^"]+)" y2="([^"]+)"/.exec(logo);
	const stops = [...logo.matchAll(/<stop offset="([^"]+)" stop-color="([^"]+)"\/>/g)];
	if (!axis || stops.length < 2) throw new Error("logo.svg has no userSpaceOnUse linearGradient with stops");
	const stopRows = stops.map((m) => `\t\t{ offset: ${Number(m[1])}, color: ${JSON.stringify(m[2])} },`).join("\n");
	return `// GENERATED by scripts/icons.ts from assets/logo.svg. Do not edit; run \`bun run icons\`.
export const MARK_VIEW = { w: ${view[1]}, h: ${view[2]} } as const;

/** The mark's fill: a linear gradient in logo units (userSpaceOnUse), for consumers that draw the tiles themselves. */
export const MARK_GRADIENT = {
	x1: ${Number(axis[1])},
	y1: ${Number(axis[2])},
	x2: ${Number(axis[3])},
	y2: ${Number(axis[4])},
	stops: [
${stopRows}
	],
} as const;

/** One entry per <path> of the logo, in file order: id = index, bounding box in logo units, and the path data. */
export const MARK: { id: number; cx: number; cy: number; w: number; h: number; d: string }[] = [
${rows}
];
`;
}

// ── rasterization ──────────────────────────────────────────────────────────────────────────────────────────
function run(cmd: string, args: string[], cwd = ROOT) {
	const r = spawnSync(cmd, args, { cwd, encoding: "utf8" });
	if (r.error || r.status !== 0) throw new Error(`${cmd} ${args.join(" ")} failed: ${r.error?.message ?? r.stderr}`);
	return r.stdout;
}
export function haveRasterTools() {
	return ["rsvg-convert", "magick", "cargo-tauri"].every((c) => spawnSync("sh", ["-c", `command -v ${c}`]).status === 0);
}

/** Render every raster from `svgRoot` (a tree holding the text outputs) into `dest` (mirrors the repo layout). */
export function renderRasters(svgRoot: string, dest: string) {
	const put = (p: string) => {
		mkdirSync(dirname(join(dest, p)), { recursive: true });
		return join(dest, p);
	};
	const svg = (p: string) => join(svgRoot, p);

	// app icon: Apple Big Sur+ template (1024 canvas, 824 plate, baked shadow) -> Tauri set via `cargo tauri icon`
	run("rsvg-convert", ["-w", "1024", "-h", "1024", svg("assets/app-icon.svg"), "-o", put("assets/app-icon-1024.png")]);
	const tauriOut = join(dest, ".tauri");
	run("cargo", ["tauri", "icon", join(dest, "assets/app-icon-1024.png"), "-o", tauriOut]);
	// `cargo tauri icon` also writes mobile / Windows-store / extra sizes we do not bundle
	// (tauri.conf.json bundle.icon: 32x32, 128x128, 128x128@2x, icon.icns; icon.ico/icon.png are the Windows/Linux set).
	for (const f of ["32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico", "icon.icns"]) copyFileSync(join(tauriOut, f), put(`crates/atlas-desktop/icons/${f}`));
	rmSync(tauriOut, { recursive: true, force: true });

	// menu bar: MUST stay monochrome (tray.rs sets icon_as_template(true); macOS tints by alpha). 44px = 22pt @2x.
	run("rsvg-convert", ["-w", "44", "-h", "44", svg("assets/tray-template.svg"), "-o", put("crates/atlas-desktop/icons/tray.png")]);
	// Linux panel: AppIndicator hosts (GNOME's extension, KDE Plasma) draw the PNG as is, with no template tinting, and
	// the panels are dark by default, so the black template would vanish. Same glyph, white, alpha kept (tray.rs picks it).
	run("magick", [join(dest, "crates/atlas-desktop/icons/tray.png"), "-channel", "RGB", "-negate", "+channel", put("crates/atlas-desktop/icons/tray-linux.png")]);

	// web: favicon.ico (16 + 32, transparent) and the touch icon (opaque white tile, mark at 130/180)
	const mark = svg("assets/logo.svg");
	for (const s of [16, 32]) run("rsvg-convert", ["-w", String(s), "-h", String(s), mark, "-o", put(`.f${s}.png`)]);
	run("magick", [join(dest, ".f16.png"), join(dest, ".f32.png"), put("site/public/favicon.ico")]);
	run("rsvg-convert", ["-w", "130", mark, "-o", put(".m130.png")]);
	run("magick", ["-size", "180x180", "xc:white", join(dest, ".m130.png"), "-gravity", "center", "-composite", "-alpha", "off", put("site/public/apple-touch-icon.png")]);
	for (const f of [".f16.png", ".f32.png", ".m130.png"]) rmSync(join(dest, f));
}

/** Decoded pixels of a raster, as a stable hash: PNG = RGBA, ICO/ICNS = every frame, sorted by size. */
export function pixelHash(file: string): string {
	const dir = mkdtempSync(join(tmpdir(), "icons-px-"));
	try {
		if (file.endsWith(".icns")) {
			run("iconutil", ["-c", "iconset", "-o", join(dir, "i.iconset"), file]);
			return sha(
				readdirSync(join(dir, "i.iconset"))
					.sort()
					.map((f) => `${f}:${rgba(join(dir, "i.iconset", f))}`)
					.join("\n"),
			);
		}
		if (file.endsWith(".ico")) {
			run("magick", [file, join(dir, "f-%d.png")]);
			return sha(
				readdirSync(dir)
					.map((f) => ({ f, s: Number(run("magick", ["identify", "-format", "%w", join(dir, f)])) }))
					.sort((a, b) => a.s - b.s)
					.map(({ f, s }) => `${s}:${rgba(join(dir, f))}`)
					.join("\n"),
			);
		}
		return rgba(file);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}
function rgba(png: string) {
	const r = spawnSync("magick", [png, "-depth", "8", "-alpha", "on", "rgba:-"], { maxBuffer: 1 << 28 });
	if (r.status !== 0) throw new Error(`magick ${png}: ${r.stderr}`);
	const wh = run("magick", ["identify", "-format", "%wx%h", png]);
	return `${wh}:${sha(r.stdout)}`;
}

// ── lock ──────────────────────────────────────────────────────────────────────────────────────────────────
export type Lock = { sources: Record<string, string>; rasters: Record<string, string> };
export function sourceHashes(): Record<string, string> {
	return Object.fromEntries([...SOURCES, "scripts/icons.ts"].map((p) => [p, sha(readFileSync(abs(p)))]));
}

/** What is wrong with the checked-in assets, one line per problem. Empty = up to date. */
export function drift({ rasters }: { rasters: boolean }): string[] {
	const bad: string[] = [];
	for (const [p, want] of textOutputs()) if (!existsSync(abs(p)) || read(p) !== want) bad.push(`${p}: not what scripts/icons.ts generates from assets/logo.svg`);

	const lock: Lock | null = existsSync(abs(LOCK)) ? JSON.parse(read(LOCK)) : null;
	if (!lock) return [...bad, `${LOCK}: missing`];
	const now = sourceHashes();
	for (const [p, h] of Object.entries(now)) if (lock.sources[p] !== h) bad.push(`${p}: changed since the rasters were rendered (run bun run icons)`);
	for (const p of RASTERS) if (!existsSync(abs(p))) bad.push(`${p}: missing`);
	for (const p of Object.keys(lock.rasters)) if (!RASTERS.includes(p)) bad.push(`${LOCK}: lists ${p}, which is not a derived raster`);

	if (rasters && bad.length === 0) {
		for (const p of RASTERS) if (lock.rasters[p] !== pixelHash(abs(p))) bad.push(`${p}: pixels differ from the lock (edited by hand or re-rendered with other tools; run bun run icons)`);
		const tmp = mkdtempSync(join(tmpdir(), "icons-check-"));
		try {
			const svgRoot = join(tmp, "svg");
			for (const [p, t] of textOutputs()) {
				mkdirSync(dirname(join(svgRoot, p)), { recursive: true });
				writeFileSync(join(svgRoot, p), t);
			}
			copyFileSync(abs("assets/logo.svg"), (mkdirSync(join(svgRoot, "assets"), { recursive: true }), join(svgRoot, "assets/logo.svg")));
			renderRasters(svgRoot, join(tmp, "out"));
			for (const p of RASTERS) if (pixelHash(abs(p)) !== pixelHash(join(tmp, "out", p))) bad.push(`${p}: stale, a fresh render of the sources looks different (run bun run icons)`);
		} finally {
			rmSync(tmp, { recursive: true, force: true });
		}
	}
	return bad;
}

function generate() {
	if (!haveRasterTools()) throw new Error("rsvg-convert, magick (ImageMagick) and cargo-tauri are required: brew install librsvg imagemagick && cargo install tauri-cli --locked");
	for (const [p, t] of textOutputs()) {
		mkdirSync(dirname(abs(p)), { recursive: true });
		writeFileSync(abs(p), t);
	}
	renderRasters(ROOT, ROOT);
	const lock: Lock = { sources: sourceHashes(), rasters: Object.fromEntries(RASTERS.map((p) => [p, pixelHash(abs(p))])) };
	writeFileSync(abs(LOCK), `${JSON.stringify(lock, null, "\t")}\n`);
	console.log(`icons: ${textOutputs().size} text + ${RASTERS.length} raster assets regenerated from assets/logo.svg`);
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	if (args.includes("--check")) {
		const text = args.includes("--text");
		if (!text && !haveRasterTools()) {
			console.error("icons: rsvg-convert/magick/cargo-tauri missing, cannot re-render; pass --text for the tool-free check");
			process.exit(2);
		}
		const bad = drift({ rasters: !text });
		if (bad.length) {
			console.error(`icons: ${bad.length} derived asset(s) out of date:\n${bad.map((b) => `  ${b}`).join("\n")}\nrun: bun run icons`);
			process.exit(1);
		}
		console.log(`icons: up to date${text ? " (text + lock)" : ""}`);
	} else generate();
}
