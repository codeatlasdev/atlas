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

/** `selector[::pseudo]` of every element under `root` whose winning `animation` loops forever. */
export function infiniteAnimations(root: Element): string[] {
	const rules = [...document.styleSheets].flatMap((s) => [...rulesOf(s.cssRules)]);
	const found: string[] = [];
	const els = [root, ...root.querySelectorAll("*")];
	for (const el of els) {
		for (const pseudo of ["", "::before", "::after"]) {
			let win: Win | undefined;
			rules.forEach((rule, order) => {
				const text =
					rule.style.getPropertyValue("animation") || rule.style.getPropertyValue("animation-name");
				if (!text) return;
				for (const sel of rule.selectorText.split(/,(?![^(]*\))/).map((x) => x.trim())) {
					if ((PSEUDO.exec(sel)?.[0] ?? "") !== pseudo) continue;
					const plain = sel.replace(PSEUDO, "");
					if (!el.matches(plain)) continue;
					const spec = specificity(sel);
					if (!win || spec > win.spec || (spec === win.spec && order > win.order))
						win = { spec, order, infinite: /\binfinite\b/.test(text), text };
				}
			});
			if (win?.infinite)
				found.push(
					`${el.tagName.toLowerCase()}.${[...el.classList].join(".")}${pseudo} {animation: ${win.text}}`,
				);
		}
	}
	return found;
}
