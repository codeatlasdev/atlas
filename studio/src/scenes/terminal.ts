import type { TerminalScene } from "../lib/types.ts";
import { placeBox } from "./app.ts";
import { type BuildCtx, type SceneOutput, titleCss, titleHtml } from "./base.ts";

/**
 * TerminalTake: a vhs take, cut to the dense segments by `prepare` (trim + concat + speed baked into the proxy), placed
 * centered on the field at the film's frame geometry.
 */
export function buildTerminal(s: TerminalScene, ctx: BuildCtx, proxy: { file: string; aspect: number }): SceneOutput {
	const b = placeBox({ x: "center", y: s.y ?? "center", w: s.width }, ctx.aspect, proxy.aspect);
	const src = ctx.asset(proxy.file);
	const overlay = s.overlay ? titleHtml("overlay", s.overlay, ctx, 48) : "";
	return {
		html: `<div id="clip" style="left:${b.x}px;top:${b.y}px;width:${b.w}px;height:${b.h}px"><video id="vid" class="clip" src="${src}" muted playsinline data-start="0" data-duration="${s.duration}" data-track-index="1" style="width:${b.w}px;height:${b.h}px"></video></div>${overlay}`,
		css: `${titleCss}
#clip{position:absolute;overflow:hidden}
#clip video{position:absolute;left:0;top:0;display:block;object-fit:fill}`,
		js: "",
	};
}
