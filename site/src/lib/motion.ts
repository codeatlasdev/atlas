// The one motion runtime for the landing page: GSAP + ScrollTrigger, Lenis as the only smooth-scroll engine.
// Under prefers-reduced-motion nothing here runs; the HTML/CSS is already in its final state.
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import Lenis from "lenis";

gsap.registerPlugin(ScrollTrigger);

export const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
/** Phones and tablets: native scroll only (no Lenis, no pinning, no scrubbed transforms). Mirrors the `touch` class set in Base.astro. */
export const touch = () => matchMedia("(pointer: coarse)").matches || matchMedia("(max-width: 900px)").matches;
export { gsap, ScrollTrigger };

let lenis: Lenis | undefined;
let tick: ((t: number) => void) | undefined;

/** Starts Lenis and wires it to ScrollTrigger. Safe to call once per page. */
export function startScroll(): Lenis | undefined {
	if (reduced() || touch() || lenis) return lenis;
	lenis = new Lenis({ lerp: 0.11, wheelMultiplier: 1, smoothWheel: true, anchors: { offset: -64 } });
	lenis.on("scroll", ScrollTrigger.update);
	tick = (t) => lenis?.raf(t * 1000);
	gsap.ticker.add(tick);
	gsap.ticker.lagSmoothing(0);
	return lenis;
}

export function stopScroll() {
	if (tick) gsap.ticker.remove(tick);
	lenis?.destroy();
	lenis = undefined;
	tick = undefined;
}

export const scroller = () => lenis;

/** Splits a heading into words for the rise reveal. The unsplit text stays as the accessible name. */
export function splitWords(el: HTMLElement): HTMLElement[] {
	if (el.dataset.split) return [...el.querySelectorAll<HTMLElement>(".w > span")];
	const text = el.textContent?.trim() ?? "";
	if (!text) return [];
	el.setAttribute("aria-label", text);
	el.dataset.split = "1";
	el.textContent = "";
	const inner: HTMLElement[] = [];
	text.split(/\s+/).forEach((word, i, all) => {
		const w = document.createElement("span");
		w.className = "w";
		w.setAttribute("aria-hidden", "true");
		const s = document.createElement("span");
		s.textContent = word;
		w.append(s);
		el.append(w);
		if (i < all.length - 1) el.append(document.createTextNode(" "));
		inner.push(s);
	});
	return inner;
}

/** Word-by-word lift for every `.rise` heading as it enters the viewport. */
export function riseHeadings() {
	document.querySelectorAll<HTMLElement>(".rise").forEach((h) => {
		const words = splitWords(h);
		if (!words.length) return;
		gsap.set(words, { yPercent: 108 });
		ScrollTrigger.create({
			trigger: h,
			start: "top 88%",
			once: true,
			onEnter: () => gsap.to(words, { yPercent: 0, duration: 0.85, ease: "power4.out", stagger: 0.045 }),
		});
	});
}
