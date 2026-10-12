// What every scene builder returns, and the pieces they share: the cobalt field, fades, the title text block and the
// Atlas mark. A builder emits markup + CSS + a GSAP snippet that runs in scene-local time on a paused timeline `tl`.

import { TEXT_SHADOW } from "../brand/tokens.ts";
import type { Aspect, SceneBase, TitleText, Mark } from "../lib/types.ts";

export interface BuildCtx {
	aspect: Aspect;
	fps: number;
	/** register a source file for the project; returns the project-relative url to use in markup */
	asset: (path: string) => string;
	/** escape for HTML text */
	esc: (s: string) => string;
}

export interface SceneOutput {
	/** markup inside #root (the field and fades are added by the compiler) */
	html: string;
	css: string;
	/** GSAP statements; `tl` is the scene's paused timeline, `D` its duration in seconds */
	js: string;
	/** async setup that must finish before the timeline is registered (e.g. none for most scenes) */
	ready?: string;
}

/** Default dip: the film fades each scene in and out through the field. */
export const DIP = 0.25;

/** Opacity fades of the scene root, in seconds. Returns GSAP statements on `#root`. */
export function fadesJs(s: SceneBase): string {
	const t = s.transitionIn ?? { type: "dip", duration: DIP };
	const out = s.fadeOut ?? DIP;
	const parts: string[] = [];
	if (t.type !== "cut") parts.push(`tl.fromTo("#root-fade",{opacity:0},{opacity:1,duration:${t.duration},ease:"none"},0);`);
	if (out > 0) parts.push(`tl.to("#root-fade",{opacity:0,duration:${out},ease:"none"},${+(s.duration - out).toFixed(4)});`);
	return parts.join("\n");
}

/** The cobalt field: a cover image with an optional slow push-in (scale grows by `zoom` per frame at 30 fps). */
export function fieldHtml(src: string): string {
	return `<div id="field" aria-hidden="true"><div id="field-img" style="background-image:url(${src})"></div></div>`;
}

export const fieldCss = `#field{position:absolute;inset:0;overflow:hidden;background:#0b1672}
#field-img{position:absolute;inset:0;background-size:cover;background-position:center;transform-origin:50% 50%}`;

export function fieldJs(zoom: number, duration: number): string {
	if (!zoom) return "";
	const to = +(1 + zoom * 30 * duration).toFixed(5);
	return `tl.fromTo("#field-img",{scale:1},{scale:${to},duration:${duration},ease:"none"},0);`;
}

/** Title block markup: centered headline, subtitle, optional mono url. Positioned by `y` from the top. */
export function titleHtml(id: string, t: TitleText, ctx: BuildCtx, subtitleSize: number): string {
	const sub = t.subtitle ? `<div class="sub" style="font-size:${t.subtitleSize ?? subtitleSize}px">${ctx.esc(t.subtitle)}</div>` : "";
	const url = t.url ? `<div class="url">${ctx.esc(t.url)}</div>` : "";
	const wrap = t.maxWidth ? ` wrap" style="top:${t.y}px;width:${t.maxWidth}px` : `" style="top:${t.y}px`;
	return `<div id="${id}" class="titleblock${wrap}"><div class="h" style="font-size:${t.titleSize}px">${ctx.esc(t.title)}</div>${sub}${url}</div>`;
}

export const titleCss = `.titleblock{position:absolute;left:0;right:0;text-align:center;color:#fff;font-family:var(--font)}
.titleblock .h{font-weight:700;letter-spacing:-.035em;text-shadow:${TEXT_SHADOW}}
.titleblock .sub{margin-top:22px;font-weight:400;color:rgba(255,255,255,.78);text-shadow:${TEXT_SHADOW}}
.titleblock .url{margin-top:44px;font:500 34px var(--mono);color:#fff;opacity:.92}
.titleblock.wrap{left:50%;right:auto;transform:translateX(-50%)}
.titleblock.wrap .h,.titleblock.wrap .sub{text-wrap:balance}`;

/** Fade-in of an element from alpha 0; `delay`/`fade` in seconds. */
export function alphaIn(sel: string, delay: number, fade: number): string {
	return `tl.fromTo("${sel}",{opacity:0},{opacity:1,duration:${fade},ease:"none"},${delay});`;
}

/** The Atlas mark: the real logo SVG, centered horizontally at `y`. */
export function markHtml(m: Mark, ctx: BuildCtx): string {
	const src = ctx.asset("assets/logo/logo.svg");
	return `<img id="mark" src="${src}" alt="" style="position:absolute;left:50%;top:${m.y}px;width:${m.size}px;margin-left:${-m.size / 2}px">`;
}

export function markJs(m: Mark): string {
	return alphaIn("#mark", m.delay ?? 0, m.fade ?? 0.3);
}
