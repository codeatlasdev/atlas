// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Which `infinite` CSS animations a rendered tree would run. WebKitGTK on Linux keeps its whole compositing pipeline at
 * 60 fps for ONE running animation (docs/perf/linux-webkit-study.md), so a settled screen must have none.
 *
 * happy-dom does not resolve a cascade (`getComputedStyle` returns nothing for pseudo-elements), so this does the part
 * that matters here: the component's real compiled CSS (and `app.css`) goes into the document, and every rendered element
 * (and its `::before` / `::after`) is matched against every rule; the winner by specificity then order decides.
 * `prefers-reduced-motion: reduce` blocks are skipped (that is the reduced case, tested by the global rule in app.css).
 */
import { compile } from "svelte/compiler";

const PSEUDO = /::(?:before|after)$/;

/** Specificity of one compound selector list item (ids, classes/attributes/pseudo-classes, elements); `:where()` counts 0. */
function specificity(sel: string): number {
	const s = sel.replace(/:where\([^)]*\)/g, "").replace(PSEUDO, "");
	const ids = (s.match(/#[\w-]+/g) ?? []).length;
	const cls = (s.match(/\.[\w-]+|\[[^\]]*\]|:(?!:)[\w-]+/g) ?? []).length;
	const els = (s.match(/(?:^|[\s>+~])[a-z][\w-]*/gi) ?? []).length;
	return ids * 10000 + cls * 100 + els;
}

type Win = { spec: number; order: number; infinite: boolean; text: string };

function* rulesOf(list: CSSRuleList): Generator<CSSStyleRule> {
	for (const r of list) {
		if (r.cssRules && r.media) {
			if (/prefers-reduced-motion:\s*reduce/.test(r.conditionText ?? r.media.mediaText ?? "")) continue;
			yield* rulesOf(r.cssRules);
		} else if (r.selectorText && r.style) yield r;
	}
}

/** Add the compiled `<style>` of each component (by absolute path) and the app's global CSS to the document. */
export async function loadStyles(componentPaths: string[], globalCss: string[] = []) {
	const sheets: HTMLStyleElement[] = [];
	for (const p of componentPaths) {
		const out = compile(await Bun.file(p).text(), { filename: p, generate: "client", dev: true });
		if (out.css) sheets.push(Object.assign(document.createElement("style"), { textContent: out.css.code }));
	}
	for (const p of globalCss)
		sheets.push(Object.assign(document.createElement("style"), { textContent: await Bun.file(p).text() }));
	document.head.append(...sheets);
	return () => sheets.forEach((s) => s.remove());
}

/** Winning declaration text of `props` (the first that any rule sets) per element and pseudo, as `[label, text]`. */
function winners(root: Element, props: string[]): [string, string][] {
	const rules = [...document.styleSheets].flatMap((s) => [...rulesOf(s.cssRules)]);
	const out: [string, string][] = [];
	for (const el of [root, ...root.querySelectorAll("*")]) {
		for (const pseudo of ["", "::before", "::after"]) {
			let win: Win | undefined;
			rules.forEach((rule, order) => {
				const text = props.map((p) => rule.style.getPropertyValue(p)).find(Boolean);
				if (!text) return;
				for (const sel of rule.selectorText.split(/,(?![^(]*\))/).map((x) => x.trim())) {
					if ((PSEUDO.exec(sel)?.[0] ?? "") !== pseudo) continue;
					if (!el.matches(sel.replace(PSEUDO, ""))) continue;
					const spec = specificity(sel);
					if (!win || spec > win.spec || (spec === win.spec && order > win.order))
						win = { spec, order, infinite: false, text };
				}
			});
			if (win) out.push([`${el.tagName.toLowerCase()}.${[...el.classList].join(".")}${pseudo}`, win.text]);
		}
	}
	return out;
}

/** `selector[::pseudo] {animation: ...}` of every element whose winning `animation` loops forever. */
export function infiniteAnimations(root: Element): string[] {
	return winners(root, ["animation", "animation-name"])
		.filter(([, text]) => /\binfinite\b/.test(text))
		.map(([label, text]) => `${label} {animation: ${text}}`);
}

/** Elements whose winning `transition` animates `prop` (or `all`) for a non-zero time: a change of it runs an animation. */
export function transitionsOf(root: Element, prop: string): string[] {
	return winners(root, ["transition"])
		.filter(([, text]) =>
			text.split(/,(?![^(]*\))/).some((t) => {
				const [name, dur] = t.trim().split(/\s+/);
				return (name === prop || name === "all") && parseFloat(dur) > 0;
			}),
		)
		.map(([label, text]) => `${label} {transition: ${text}}`);
}
