import { createHighlighter, type Highlighter } from "shiki";
import { CODE_THEME } from "../brand/tokens.ts";
import type { CodeScene } from "../lib/types.ts";
import { alphaIn, type BuildCtx, type SceneOutput, titleCss } from "./base.ts";

let highlighter: Promise<Highlighter> | undefined;

/**
 * CodeCard: a window with highlighted code (shiki, the cobalt theme). With `typing`, characters are revealed by a
 * seeked GSAP tween over the highlighted tokens, so the animation is deterministic and the final frame is the full code.
 */
export async function buildCode(s: CodeScene, ctx: BuildCtx): Promise<SceneOutput> {
	highlighter ??= createHighlighter({ themes: [CODE_THEME], langs: [s.lang] });
	const h = await highlighter;
	if (!h.getLoadedLanguages().includes(s.lang)) await h.loadLanguage(s.lang as never);
	// one span per character so typing can reveal them without re-flowing
	const html = h
		.codeToHtml(s.code, { lang: s.lang, theme: CODE_THEME.name })
		.replace(/(<span[^>]*>)([^<]+)(<\/span>)/g, (_m, open: string, text: string, close: string) =>
			`${open}${[...text].map((c) => `<i class="ch">${c === " " ? "&nbsp;" : c}</i>`).join("")}${close}`
		);
	const typing = s.typing
		? `const chars=gsap.utils.toArray("#code .ch");chars.forEach(c=>{c.style.opacity="0"});` +
			`tl.to(chars,{opacity:1,duration:0.001,stagger:${(1 / s.typing.cps).toFixed(5)},ease:"none"},${s.typing.startAt});`
		: "";
	return {
		html: `<div id="heading" class="titleblock" style="top:${s.heading.y}px"><div class="h" style="font-size:${s.heading.size}px">${ctx.esc(s.heading.text)}</div></div>
<div id="card" style="top:${s.y}px;width:${s.width}px;margin-left:${-s.width / 2}px"><div class="file">${ctx.esc(s.file)}</div><div id="code">${html}</div></div>`,
		css: `${titleCss}
#card{position:absolute;left:50%;box-sizing:border-box;background:rgba(6,12,70,.88);border:1px solid rgba(255,255,255,.22);border-radius:18px;padding:22px 44px 26px;box-shadow:0 30px 120px rgba(5,10,60,.6);color:#eef0f8}
#card .file{font:500 20px var(--mono);color:rgba(255,255,255,.55);margin-bottom:10px}
#code pre.shiki{margin:0;background:transparent!important;font:500 ${s.fontSize}px/1.36 var(--mono);white-space:pre}
#code code{display:block}`,
		js: [s.delay !== undefined ? alphaIn("#card", s.delay, s.fade ?? 0.5) : "", typing].filter(Boolean).join("\n"),
	};
}
