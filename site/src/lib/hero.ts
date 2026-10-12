// Hero boot: the SSR'd board is complete (final state). With motion allowed we rewind it and play the boot once:
// atlas.yaml types in, the graph boots wave by wave, ports light up, worker crashes once and restarts,
// then an agent forks off a worktree and boots its own copy on ports +100. Nav, headline and the waitlist
// field are never hidden, so the page is usable from frame 0.
//
// The board exists twice in the DOM: the landscape graph (desktop) and a portrait vertical chain (phones, `p:` ids).
// Both are driven by the same timeline; CSS shows one. On phones only one pane is visible at a time: the yaml types
// first (`.stage-file`), then the graph crossfades in.
import { gsap, reduced } from "./motion";

export function playHero(): (() => void) | undefined {
	const root = document.querySelector<HTMLElement>(".hero");
	if (!root || reduced()) return;
	const q = <T extends Element>(s: string) => [...root.querySelectorAll<T>(s)];
	const lines = q<HTMLElement>(".yl");
	const pre = root.querySelector<HTMLElement>(".file pre");
	const nodes = q<SVGGElement>(".graph .node");
	const edges = q<SVGPathElement>(".graph [data-edge]");
	const agents = q<SVGGElement>("[data-agent]");
	const count = root.querySelector<HTMLElement>("[data-count]");
	const sum = root.querySelector<HTMLElement>("[data-sum]");
	const phone = matchMedia("(max-width: 960px)");
	// Each id resolves to both layouts, so one timeline drives whichever one is on screen.
	const node = (id: string) => q<SVGGElement>(`[data-node="${id}"], [data-node="p:${id}"]`);
	const edge = (id: string) => q<SVGPathElement>(`[data-edge="${id}"], [data-edge="p:${id}"]`);
	const setState = (g: Element | Element[], s: "off" | "wait" | "up" | "fail") => {
		for (const el of Array.isArray(g) ? g : [g]) {
			el.classList.remove("off", "wait", "up", "fail");
			el.classList.add(s);
		}
	};
	const setCount = (n: number, total = 5) => {
		if (count) count.textContent = String(n);
		if (sum) sum.lastChild!.textContent = ` of ${total} up`;
		const dot = sum?.querySelector(".dot");
		if (dot) dot.className = `dot ${n === total ? "go" : "wait"}`;
	};

	// Rewind to the starting frame.
	const textNodes = lines.map((l) => ({ el: l, full: l.textContent ?? "" }));
	gsap.set(textNodes.map((t) => t.el), { opacity: 1 });
	textNodes.forEach((t) => (t.el.textContent = ""));
	nodes.forEach((n) => setState(n, "off"));
	gsap.set(nodes, { opacity: 0.35 });
	gsap.set(edges, { strokeDashoffset: 1 });
	gsap.set(agents, { opacity: 0 });
	setCount(0);
	root.classList.add("playing");
	if (phone.matches) root.classList.add("stage-file");

	const tl = gsap.timeline({ delay: 0.5, defaults: { ease: "none" } });

	// 1. type the file, line by line. On phones the pane only fits ~16 lines, so it follows the cursor.
	const typed = { n: 0 };
	tl.to(typed, {
		n: textNodes.length,
		duration: 3.1,
		onUpdate() {
			const n = Math.floor(typed.n);
			for (let i = 0; i < textNodes.length; i++) {
				const want = i < n ? textNodes[i].full : "";
				if (textNodes[i].el.textContent !== want) textNodes[i].el.textContent = want;
			}
			if (pre && phone.matches) pre.scrollTop = pre.scrollHeight;
		},
	});
	// Phones: hold the finished file a beat, then hand the stage to the graph.
	tl.call(() => root.classList.remove("stage-file"), [], 3.55);

	// 2. boot waves. Each step: waiting -> starting -> healthy, edge draws toward the next node.
	const boot = (id: string, at: number, startAfter = 0.35, to: "up" | "fail" = "up") => {
		const g = node(id);
		tl.set(g, { opacity: 1 }, at);
		tl.call(() => setState(g, "wait"), [], at);
		tl.call(() => setState(g, to), [], at + startAfter);
	};
	let up = 0;
	const bump = (at: number) => tl.call(() => setCount(++up), [], at);

	// Phones start the boot after the crossfade (3.55 + 0.6 fade), desktop keeps its overlap with the typing.
	const t0 = phone.matches ? 4.0 : 2.4;
	const at = (t: number) => t - 2.4 + t0;
	tl.add("boot", t0);
	boot("database", at(2.4), 0.5);
	bump(at(2.9));
	tl.to(edge("database-migrate"), { strokeDashoffset: 0, duration: 0.45 }, at(2.9));
	boot("migrate", at(3.3), 0.6);
	bump(at(3.9));
	tl.to(edge("migrate-api"), { strokeDashoffset: 0, duration: 0.45 }, at(3.9));
	boot("api", at(4.3), 0.9);
	bump(at(5.2));
	tl.to([...edge("api-web"), ...edge("api-worker")], { strokeDashoffset: 0, duration: 0.5 }, at(5.2));
	boot("web", at(5.6), 0.6);
	bump(at(6.2));
	// worker crashes once, backs off, restarts: that is what the product does
	boot("worker", at(5.6), 0.5, "fail");
	tl.call(() => setCount(up, 5), [], at(6.1));
	tl.call(() => setState(node("worker"), "wait"), [], at(7.0));
	tl.call(() => setState(node("worker"), "up"), [], at(7.5));
	bump(at(7.5));

	// 3. an agent forks a worktree and boots its own copy on ports +100
	tl.to(agents, { opacity: 1, duration: 0.01 }, at(8.3));
	tl.to(edge("fork"), { strokeDashoffset: 0, duration: 0.7 }, at(8.3));
	const mini = ["agent-api", "agent-web"].map(node);
	mini.forEach((g, i) => {
		tl.set(g, { opacity: 1 }, at(9.0) + i * 0.4);
		tl.call(() => setState(g, "wait"), [], at(9.0) + i * 0.4);
		tl.call(() => setState(g, "up"), [], at(9.5) + i * 0.4);
	});
	mini.forEach((g) => {
		setState(g, "off");
		gsap.set(g, { opacity: 0.35 });
	});
	gsap.set(edge("fork"), { strokeDashoffset: 1 });

	return () => {
		tl.kill();
		root.classList.remove("playing", "stage-file");
	};
}
