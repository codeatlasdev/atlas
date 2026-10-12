import type { TitleScene } from "../lib/types.ts";
import { alphaIn, type BuildCtx, markHtml, markJs, type SceneOutput, titleCss, titleHtml } from "./base.ts";

/** TitleCard (headline, subtitle, url) with an optional LogoReveal (the Atlas mark fading in above the text). */
export function buildTitle(s: TitleScene, ctx: BuildCtx): SceneOutput {
	const sub = ctx.aspect.id === "16:9" ? 44 : 48;
	const t = s.text;
	return {
		html: `${s.mark ? markHtml(s.mark, ctx) : ""}${titleHtml("text", t, ctx, sub)}`,
		css: titleCss,
		js: [s.mark ? markJs(s.mark) : "", t.delay !== undefined ? alphaIn("#text", t.delay, t.fade ?? 0.4) : ""].filter(Boolean).join("\n"),
	};
}
