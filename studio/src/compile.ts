// Compiles one cut of a film into a HyperFrames project under studio/.build/<film>/<aspect>/:
//   index.html                 the host: one timed clip per scene (+ overlays and burned captions), black stage
//   compositions/<scene>.html  one sub-composition per scene (its own paused GSAP timeline in scene-local time)
//   assets/…                   fonts, logo, backgrounds, proxies copied in (the project is self-contained)
// The output is a pure function of the film data and the asset hashes: no timestamps, stable ordering. The render is
// silent; audio is mixed afterwards (src/audio/mix.ts), where the loudness is measured.

import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { FONT, ROOT_VARS } from "./brand/tokens.ts";
import { assetPath } from "./lib/assets.ts";
import { sha256 } from "./lib/exec.ts";
import { BUILD, STUDIO, StudioError } from "./lib/paths.ts";
import { ASPECTS, type Cut, type Film, type Overlay, type Scene } from "./lib/types.ts";
import { prepareApp, prepareTerminal, type Proxy } from "./prepare.ts";
import { buildApp, placeBox } from "./scenes/app.ts";
import { type BuildCtx, fadesJs, fieldCss, fieldHtml, fieldJs, type SceneOutput } from "./scenes/base.ts";
import { buildCode } from "./scenes/code.ts";
import { buildTerminal } from "./scenes/terminal.ts";
import { buildTitle } from "./scenes/title.ts";

const GSAP = "https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js";

export interface Compiled {
	dir: string;
	/** total duration, seconds */
	duration: number;
	/** hash of index.html + compositions + the hashes of copied assets */
	hash: string;
	/** start second of each scene */
	starts: Record<string, number>;
	width: number;
	height: number;
}

export interface CompileOpts {
	/** clamp every scene to this many seconds (the `check` smoke render) */
	clampSeconds?: number;
	/** build into this dir instead of .build/<film>/<aspect> */
	dir?: string;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const num = (n: number) => +n.toFixed(4);

export async function compile(film: Film, cut: Cut, opts: CompileOpts = {}): Promise<Compiled> {
	const aspect = ASPECTS[cut.aspect];
	const dir = opts.dir ?? join(BUILD, film.id, cut.aspect.replace(":", "x"));
	rmSync(dir, { recursive: true, force: true });
	mkdirSync(join(dir, "compositions"), { recursive: true });
	mkdirSync(join(dir, "assets"), { recursive: true });

	const copied = new Map<string, string>();
	const asset = (path: string): string => {
		const name = path.startsWith("assets/") || path.startsWith("proxy-") ? path : `assets/${basename(path)}`;
		if (!copied.has(name)) {
			const from = path.startsWith("/") ? path : join(STUDIO, path);
			const to = join(dir, name);
			mkdirSync(dirname(to), { recursive: true });
			if (!existsSync(from)) throw new StudioError(`asset not found: ${path}`);
			copyFileSync(from, to);
			copied.set(name, from);
		}
		return name.startsWith("proxy-") ? `assets/${name}` : name;
	};
	// proxies are copied under assets/ with their proxy- name
	const ctx: BuildCtx = { aspect, fps: film.fps, esc, asset };

	const scenes = opts.clampSeconds ? cut.scenes.map((s) => ({ ...s, duration: Math.min(s.duration, opts.clampSeconds!) })) : cut.scenes;

	// timeline: a crossfade overlaps the previous scene by its transition duration
	const starts: Record<string, number> = {};
	let t = 0;
	for (const s of scenes) {
		const x = s.transitionIn;
		if (x?.type === "crossfade") t = Math.max(0, t - x.duration);
		starts[s.id] = num(t);
		t += s.duration;
	}
	const duration = num(t);

	const hosts: string[] = [];
	for (const [i, s] of scenes.entries()) {
		const out = await buildScene(s, ctx, film, dir, copied);
		writeFileSync(join(dir, "compositions", `${s.id}.html`), sceneFile(s, out, ctx));
		hosts.push(
			`<div id="${s.id}" class="clip" data-composition-id="${s.id}" data-composition-src="compositions/${s.id}.html" data-start="${starts[s.id]}" data-duration="${s.duration}" data-track-index="${i % 2}" data-width="${aspect.w}" data-height="${aspect.h}"></div>`
		);
	}

	const overlays = (cut.overlays ?? []).map((o, i) => overlayHtml(o, i, aspect.w));
	const captions = cut.captions.burned && cut.voice ? burnedCaptions(cut, aspect.w, aspect.h) : "";
	copyFont(dir);
	writeFileSync(join(dir, "index.html"), hostFile(film, cut, aspect.w, aspect.h, duration, hosts, overlays, captions));

	const hash = sha256(
		[readFileSync(join(dir, "index.html"), "utf8"), ...scenes.map((s) => readFileSync(join(dir, "compositions", `${s.id}.html`), "utf8")), ...[...copied.keys()].sort().map((k) => `${k}:${sha256(readFileSync(join(dir, k)))}`)].join("\n")
	).slice(0, 16);
	return { dir, duration, hash, starts, width: aspect.w, height: aspect.h };
}

async function buildScene(s: Scene, ctx: BuildCtx, film: Film, dir: string, copied: Map<string, string>): Promise<SceneOutput> {
	switch (s.type) {
		case "title":
			return buildTitle(s, ctx);
		case "code":
			return buildCode(s, ctx);
		case "app": {
			const b = placeBox(s.box, ctx.aspect, s.crop.w / s.crop.h);
			const proxy = await prepareApp(s, b.w, b.h, film.fps);
			return buildApp(s, ctx, stage(proxy, dir, copied));
		}
		case "terminal": {
			const proxy = await prepareTerminal(s, film.fps);
			return buildTerminal(s, ctx, stage(proxy, dir, copied));
		}
	}
}

/** Copy a proxy into the project and return the descriptor the builders use. */
function stage(p: Proxy, dir: string, copied: Map<string, string>): { file: string; aspect: number } {
	const rel = `assets/${p.file}`;
	if (!copied.has(rel)) {
		copyFileSync(p.path, join(dir, rel));
		copied.set(rel, p.path);
	}
	return { file: rel, aspect: p.aspect };
}

function copyFont(dir: string): void {
	for (const f of Object.values(FONT.files)) {
		mkdirSync(join(dir, dirname(f)), { recursive: true });
		copyFileSync(join(STUDIO, f), join(dir, f));
	}
}

const fontFaces = `@font-face{font-family:"Archivo";src:url(assets/fonts/archivo-var.woff2) format("woff2");font-weight:100 900;font-stretch:62% 125%}
@font-face{font-family:"JetBrains Mono";src:url(assets/fonts/jetbrains-mono-var.woff2) format("woff2");font-weight:100 800}`;

function sceneFile(s: Scene, out: SceneOutput, ctx: BuildCtx): string {
	const bgKey = `bg:${s.bg ?? "a"}`;
	const bg = ctx.asset(assetPath(bgKey));
	const D = s.duration;
	return `<!doctype html>
<html><head><meta charset="utf-8"></head><body>
<template>
<style>
${fontFaces}
#root{position:absolute;inset:0;overflow:hidden;${ROOT_VARS.split(";").join(";")}}
#root-fade{position:absolute;inset:0}
${fieldCss}
${out.css}
</style>
<div id="root" data-composition-id="${s.id}" data-width="${ctx.aspect.w}" data-height="${ctx.aspect.h}">
<div id="root-fade">
${fieldHtml(bg)}
${out.html}
</div>
</div>
<script>
const D=${D};
const tl=gsap.timeline({paused:true});
${fieldJs(s.zoom ?? 0, D)}
${out.js}
${fadesJs(s)}
window.__timelines["${s.id}"]=tl;
</script>
</template>
</body></html>
`;
}

function overlayHtml(o: Overlay, i: number, w: number): string {
	const common = `id="ov${i}" class="clip" data-start="${o.at}" data-duration="${o.duration}" data-track-index="${2 + (i % 2)}"`;
	if (o.type === "chapter") {
		return `<div ${common} style="position:absolute;left:${Math.round(w * 0.04)}px;top:48px;color:#fff;font:600 28px var(--mono);letter-spacing:.04em;text-shadow:0 2px 20px rgba(5,10,60,.8)">${String(o.index).padStart(2, "0")} / ${esc(o.title)}</div>`;
	}
	return `<div ${common} style="position:absolute;left:${Math.round(w * 0.05)}px;bottom:96px;color:#fff;font-family:var(--font);text-shadow:0 2px 24px rgba(5,10,60,.85)"><div style="font-size:48px;font-weight:700;letter-spacing:-.02em">${esc(o.title)}</div>${o.subtitle ? `<div style="font-size:28px;opacity:.8;margin-top:6px">${esc(o.subtitle)}</div>` : ""}</div>`;
}

/** Burned captions: one timed element per narration line, at the line's start for its measured length (see mix.ts). */
function burnedCaptions(cut: Cut, w: number, h: number): string {
	const lines = cut.voice?.lines ?? [];
	return lines
		.map((l, i) => {
			const end = (lines[i + 1]?.at ?? l.at + 6) - 0.05;
			return `<div id="cap${i}" class="clip" data-start="${l.at}" data-duration="${num(Math.max(0.5, end - l.at))}" data-track-index="4" style="position:absolute;left:${Math.round(w * 0.08)}px;right:${Math.round(w * 0.08)}px;bottom:${Math.round(h * 0.12)}px;text-align:center;color:#fff;font:600 ${w > h ? 44 : 52}px/1.25 var(--font);text-shadow:0 2px 24px rgba(5,10,60,.95),0 0 6px rgba(5,10,60,.9)">${esc(l.text)}</div>`;
		})
		.join("\n");
}

function hostFile(film: Film, cut: Cut, w: number, h: number, duration: number, hosts: string[], overlays: string[], captions: string): string {
	return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=${w}, height=${h}">
<script src="${GSAP}"></script>
<style>
${fontFaces}
html,body{margin:0;background:#000}
#root{position:relative;width:100%;height:100%;overflow:hidden;background:#000;${ROOT_VARS}}
.clip{position:absolute;inset:0}
</style></head><body>
<div id="root" data-composition-id="main" data-start="0" data-width="${w}" data-height="${h}" data-duration="${duration}" data-fps="${film.fps}">
${hosts.join("\n")}
${overlays.join("\n")}
${captions}
</div>
<script>
const tl=gsap.timeline({paused:true});
window.__timelines["main"]=tl;
</script>
</body></html>
`;
}
