// Landing page choreography. Everything here is skipped under prefers-reduced-motion, where the HTML and CSS
// already render every final state (no hidden content, no scrub, no autoplay).
import { gsap, ScrollTrigger, reduced, touch, startScroll, stopScroll, riseHeadings } from "./motion";

const doc = document.documentElement;
const $$ = <T extends Element>(s: string) => [...document.querySelectorAll<T>(s)];
const calm = reduced();

// Nav gets a ground after the hero. Plain scroll checks, not ScrollTrigger ranges: a range with a computed end drops its
// class once lazy media grows the page.
const hero = document.querySelector<HTMLElement>(".hero");
if (hero) {
	const solid = () => doc.classList.toggle("nav-solid", hero.getBoundingClientRect().bottom <= 60);
	solid();
	addEventListener("scroll", solid, { passive: true });
	addEventListener("resize", solid, { passive: true });
}

// Rail: visible from the first chapter to the end of the FAQ; the chapter under the viewport's middle is current, and the
// hairline beside the icons fills with the progress through that range.
const links = $$<HTMLAnchorElement>("[data-rail]");
const chapters = $$<HTMLElement>("[data-sec]").filter((s) => links.some((a) => a.dataset.rail === s.dataset.sec));
let railCur = "";
const rail = () => {
	const first = chapters[0];
	const last = chapters[chapters.length - 1];
	if (!first || !last) return;
	const mid = innerHeight / 2;
	const top = first.getBoundingClientRect().top;
	const end = last.getBoundingClientRect().bottom;
	doc.classList.toggle("rail-on", top < innerHeight * 0.4 && end > mid);
	doc.style.setProperty("--rail-progress", Math.min(1, Math.max(0, (mid - top) / (end - top))).toFixed(3));
	const cur = chapters.find((s) => {
		const r = s.getBoundingClientRect();
		return r.top <= mid && r.bottom > mid;
	})?.dataset.sec ?? "";
	if (cur === railCur) return;
	railCur = cur;
	links.forEach((a) => (cur && a.dataset.rail === cur ? a.setAttribute("aria-current", "true") : a.removeAttribute("aria-current")));
};
rail();
addEventListener("scroll", rail, { passive: true });
addEventListener("resize", rail, { passive: true });

// Nav ink: follow the ground under the bar. Computed from the section at the top of the viewport on every scroll
// frame, so jumps (rail links, anchors, back/forward) can never leave white text on paper.
const sections = $$<HTMLElement>("main > section");
let lastInk = "";
const inkUnderBar = () => {
	const y = 28;
	const under = sections.find((s) => s.getBoundingClientRect().top <= y && s.getBoundingClientRect().bottom > y);
	const paper = !!under?.classList.contains("on-paper");
	const next = under?.id ?? "";
	if (next === lastInk) return;
	lastInk = next;
	doc.classList.toggle("on-paper-sec", paper);
	if (under) doc.style.setProperty("--bg-under", getComputedStyle(under).backgroundColor);
};
inkUnderBar();
addEventListener("scroll", inkUnderBar, { passive: true });

// Lazy clips: download on approach, play only while visible and the tab is visible.
const clips = $$<HTMLVideoElement>("video[data-clip]");
const clipIO = new IntersectionObserver(
	(entries) => {
		for (const e of entries) {
			const v = e.target as HTMLVideoElement;
			if (e.isIntersecting && !document.hidden) {
				if (!v.dataset.ready) {
					v.querySelectorAll<HTMLSourceElement>("source[data-src]").forEach((s) => (s.src = s.dataset.src!));
					v.load();
					v.dataset.ready = "1";
				}
				v.play().catch(() => {});
			} else v.pause();
		}
	},
	{ rootMargin: "120px", threshold: 0.2 },
);
// Data Saver: footage stays on its poster (the clips are decoration; the film has its own play button).
const saveData = (navigator as Navigator & { connection?: { saveData?: boolean } }).connection?.saveData === true;
if (!calm && !saveData) clips.forEach((v) => clipIO.observe(v));

const onVis = () => {
	if (document.hidden) clips.forEach((v) => v.pause());
};
document.addEventListener("visibilitychange", onVis);

if (!calm) {
	startScroll();
	riseHeadings();

	// On touch (phones, tablets) native scroll is never fought: no pin, no scrub, no Lenis. The same beats play once
	// as short reveals when the element enters the viewport.
	const native = touch();
	const once = (trigger: Element, start = "top 85%") => ({ trigger, start, once: true });

	// Agents: branches draw one by one (pinned scrub on desktop, one timed draw on touch).
	$$<SVGElement>("[data-branches]").forEach((box) => {
		const trunk = box.querySelector(".trunk");
		const bs = [...box.querySelectorAll<SVGGElement>(".b")];
		const chapter = box.closest<HTMLElement>(".screen")!;
		const tl = gsap.timeline({
			scrollTrigger: native ? once(box, "top 80%") : { trigger: chapter, start: "top top", end: "+=90%", scrub: 0.6, pin: true, anticipatePin: 1 },
		});
		gsap.set([trunk, ...bs.map((b) => b.querySelector("path"))], { strokeDashoffset: 1 });
		gsap.set(bs.map((b) => b.querySelectorAll("circle, text")), { opacity: 0 });
		tl.to(trunk, { strokeDashoffset: 0, duration: 1 });
		bs.forEach((b, i) => {
			tl.to(b.querySelectorAll("circle, text"), { opacity: 1, duration: 0.2 }, i * 0.8 + 0.2);
			tl.to(b.querySelector("path"), { strokeDashoffset: 0, duration: 0.7 }, i * 0.8 + 0.2);
		});
	});

	// The idea: the four stations come in along the route, left to right, the way the voyage reads. Once, when the route enters.
	$$<HTMLElement>("[data-route]").forEach((route) => {
		const tl = gsap.timeline({ scrollTrigger: once(route, "top 80%") });
		tl.from(route.querySelector(".sea"), { opacity: 0, duration: 0.5 });
		tl.from(route.querySelectorAll(".st"), { opacity: 0, y: 18, duration: 0.5, stagger: 0.18, ease: "power2.out" }, 0.1);
	});

	// The night: each step lights up in time order, and the question that waits for you settles last.
	$$<HTMLElement>("[data-night]").forEach((fig) => {
		const steps = fig.querySelectorAll(".nt");
		const tl = gsap.timeline({ scrollTrigger: once(fig, "top 78%") });
		tl.from(steps, { opacity: 0, x: -12, duration: 0.45, stagger: 0.35, ease: "power2.out" });
		tl.from(fig.querySelector(".badge"), { scale: 0.6, opacity: 0, duration: 0.35, ease: "back.out(2)" }, 0.35 * 2 + 0.2);
	});

	// Grand Line: the routes between the islands appear one after the other.
	$$<SVGElement>("[data-ocean]").forEach((sea) => {
		gsap.from(sea.querySelectorAll(".routes path"), { opacity: 0, duration: 0.6, stagger: 0.25, scrollTrigger: once(sea.closest("section")!, "top 70%") });
	});

	// MCP transcript types in once.
	$$<HTMLElement>("[data-type] code").forEach((code) => {
		const total = code.textContent?.length ?? 0;
		if (!total) return;
		const nodes: { n: Text; t: string }[] = [];
		const walk = document.createTreeWalker(code, NodeFilter.SHOW_TEXT);
		while (walk.nextNode()) nodes.push({ n: walk.currentNode as Text, t: walk.currentNode.textContent ?? "" });
		const state = { c: 0 };
		const render = () => {
			let left = Math.floor(state.c);
			for (const { n, t } of nodes) {
				n.textContent = t.slice(0, Math.max(0, left));
				left -= t.length;
			}
		};
		// Reserve the finished height so the page never jumps while the transcript types.
		const pre = code.parentElement as HTMLElement;
		pre.style.minHeight = `${pre.offsetHeight}px`;
		nodes.forEach(({ n }) => (n.textContent = ""));
		ScrollTrigger.create({
			trigger: code,
			start: "top 78%",
			once: true,
			onEnter: () => gsap.to(state, { c: total, duration: 3.2, ease: "none", onUpdate: render }),
		});
	});
}

// Fonts and media change layout: refresh measurements once everything settled.
document.fonts?.ready.then(() => ScrollTrigger.refresh());
addEventListener("load", () => ScrollTrigger.refresh(), { once: true });

// Cleanup on page swap / bfcache.
addEventListener(
	"pagehide",
	() => {
		stopScroll();
		clipIO.disconnect();
		document.removeEventListener("visibilitychange", onVis);
		removeEventListener("scroll", inkUnderBar);
		ScrollTrigger.getAll().forEach((t) => t.kill());
	},
	{ once: true },
);
