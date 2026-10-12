import { StudioError } from "../lib/paths.ts";
import type { AppClipScene, Aspect, Box } from "../lib/types.ts";
import { type BuildCtx, type SceneOutput, titleCss, titleHtml } from "./base.ts";

/**
 * Resolve a Box against the frame: "center" centers on the axis; `h` falls back to the source aspect, rounded to even.
 * H.264 (yuv420p) needs even dimensions, so an explicit odd `w`/`h` is an authoring error, reported here instead of
 * as an ffmpeg encoder failure later.
 */
export function placeBox(box: Box, aspect: Aspect, srcAspect: number): { x: number; y: number; w: number; h: number } {
	const h = box.h ?? 2 * Math.round(box.w / srcAspect / 2);
	if (box.w % 2 !== 0 || h % 2 !== 0) throw new StudioError(`box ${box.w}x${h} has an odd side`, "H.264 needs even dimensions: change w/h in film.ts to even numbers");
	return {
		x: box.x === "center" ? Math.round((aspect.w - box.w) / 2) : box.x,
		y: box.y === "center" ? Math.round((aspect.h - h) / 2) : box.y,
		w: box.w,
		h,
	};
}

/**
 * AppClip: a recording of the real app (crop + scale baked into the proxy by `prepare`) placed on the field. The
 * `<video>` is a direct child of the scene root; push-in and pan act on a non-timed wrapper so they stay seek-safe.
 * The optional cursor ring is a deterministic overlay (recordings capture no cursor).
 */
export function buildApp(s: AppClipScene, ctx: BuildCtx, proxy: { file: string; aspect: number }): SceneOutput {
	const b = placeBox(s.box, ctx.aspect, proxy.aspect);
	const src = ctx.asset(proxy.file);
	const radius = s.radius ?? 12;
	const push = s.push ? `tl.fromTo("#clip",{scale:${s.push.from}},{scale:${s.push.to},duration:${s.duration},ease:"none"},0);` : "";
	const pan = s.pan ? `tl.fromTo("#clip",{x:0,y:0},{x:${s.pan.x},y:${s.pan.y},duration:${s.duration},ease:"none"},0);` : "";
	const cursor = s.cursor?.length ? cursorJs(s.cursor, b) : "";
	const overlay = s.overlay ? titleHtml("overlay", s.overlay, ctx, 48) : "";
	const origin = s.push ? `${(s.push.originX ?? 0.5) * 100}% ${(s.push.originY ?? 0.5) * 100}%` : "50% 50%";
	return {
		html: `<div id="clip" style="left:${b.x}px;top:${b.y}px;width:${b.w}px;height:${b.h}px;transform-origin:${origin}">
<video id="vid" class="clip" src="${src}" muted playsinline data-start="0" data-duration="${s.duration}" data-track-index="1" style="width:${b.w}px;height:${b.h}px"></video>
${s.cursor?.length ? `<div id="ring"></div>` : ""}</div>${overlay}`,
		css: `${titleCss}
#clip{position:absolute;border-radius:${radius}px;overflow:hidden;box-shadow:0 30px 120px rgba(5,10,60,.55)}
#clip video{position:absolute;left:0;top:0;display:block;object-fit:fill}
#ring{position:absolute;width:44px;height:44px;margin:-22px 0 0 -22px;border-radius:50%;border:3px solid rgba(255,255,255,.95);box-shadow:0 0 0 6px rgba(127,233,255,.25);opacity:0;pointer-events:none}`,
		js: [push, pan, cursor].filter(Boolean).join("\n"),
	};
}

function cursorJs(keys: NonNullable<AppClipScene["cursor"]>, b: { w: number; h: number }): string {
	const out: string[] = [`tl.set("#ring",{x:${Math.round(keys[0]!.x * b.w)},y:${Math.round(keys[0]!.y * b.h)}},0);`];
	out.push(`tl.to("#ring",{opacity:1,duration:0.25},${Math.max(0, keys[0]!.t - 0.25)});`);
	for (const k of keys.slice(1)) {
		out.push(`tl.to("#ring",{x:${Math.round(k.x * b.w)},y:${Math.round(k.y * b.h)},duration:0.6,ease:"power2.inOut"},${Math.max(0, k.t - 0.6)});`);
		if (k.click) out.push(`tl.fromTo("#ring",{scale:1},{scale:1.7,duration:0.12,yoyo:true,repeat:1,ease:"power2.out"},${k.t});`);
	}
	return out.join("\n");
}
