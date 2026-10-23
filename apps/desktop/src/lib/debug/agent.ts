/**
 * The page side of the agent debug bridge (`atlas desktop debug …`): wait, click, type, accessibility tree, perf,
 * element rects and a store summary. Everything is DOM events and reads inside the webview, never OS input, so it
 * works while the window is behind others, unfocused or on a virtual display, and never touches the user's mouse,
 * keyboard or focus. Debug builds expose it as `window.__atlasDebug` (shell/session.svelte.ts).
 */

import { typeInto } from "../capture/dom.ts";
import { type Key, keyBytes } from "./keys.ts";

export { contrastAudit, nonTextAudit } from "./contrast.ts";

export interface Clock {
	now(): number;
	sleep(ms: number): Promise<void>;
}
const realClock: Clock = {
	now: () => Date.now(),
	sleep: (ms) => {
		const { promise, resolve } = Promise.withResolvers<void>();
		setTimeout(resolve, ms);
		return promise;
	},
};

const visible = (e: Element) => (e as HTMLElement).offsetParent !== null;

/**
 * The element a selector means to an agent: the first match that is visible and, when a `view` is given, on screen
 * (a slid-out panel has layout but sits at x=-260). The DOM often holds hidden or off-screen copies first.
 */
function target(selector: string, view?: { w: number; h: number }): Element {
	const all = [...document.querySelectorAll(selector)];
	if (all.length === 0) throw new Error(`no element matches ${selector}`);
	const shown = all.filter(visible);
	if (shown.length === 0) throw new Error(`${selector} is not visible`);
	if (!view) return shown[0]!;
	const onScreen = shown.find((e) => {
		const r = e.getBoundingClientRect();
		return r.x < view.w && r.x + r.width > 0 && r.y < view.h && r.y + r.height > 0;
	});
	if (!onScreen) {
		const r = shown[0]!.getBoundingClientRect();
		throw new Error(
			`${selector} is outside the viewport (x=${Math.round(r.x)} y=${Math.round(r.y)} ${Math.round(r.width)}x${Math.round(r.height)})`,
		);
	}
	return onScreen;
}

/**
 * Poll until `selector` matches a visible element, or `expr` (a JS expression) is truthy. Rejects on timeout, saying
 * what was awaited. `evalExpr` is the expression evaluator (the real one is `new Function`, injected in tests).
 */
export async function waitFor(
	what: { selector?: string | null; expr?: string | null },
	o: { timeoutMs: number; pollMs: number },
	clock: Clock = realClock,
	evalExpr: (src: string) => unknown = (src) => new Function(`return (${src})`)(),
): Promise<{ ok: true; ms: number }> {
	// The bridge sends JSON: the one not given is `null`, not absent.
	if ((what.selector == null) === (what.expr == null))
		throw new Error("wait needs exactly one of selector or expr");
	const t0 = clock.now();
	for (;;) {
		const hit = what.selector
			? [...document.querySelectorAll(what.selector)].some(visible)
			: !!(await evalExpr(what.expr!));
		const ms = clock.now() - t0;
		if (hit) return { ok: true, ms };
		if (ms >= o.timeoutMs)
			throw new Error(
				`timed out after ${o.timeoutMs} ms waiting for ${what.selector ? `selector ${what.selector}` : `expression ${what.expr}`}`,
			);
		await clock.sleep(o.pollMs);
	}
}

export type Click = { clicked: true; tag: string; text: string } | { clicked: false; reason: string };

export function clickTarget(selector: string): Click {
	const all = [...document.querySelectorAll<HTMLElement>(selector)];
	if (all.length === 0) return { clicked: false, reason: `no element matches ${selector}` };
	const el = all.find(visible);
	if (!el) return { clicked: false, reason: `${selector} is not visible` };
	el.click();
	return { clicked: true, tag: el.tagName.toLowerCase(), text: (el.textContent ?? "").trim().slice(0, 80) };
}

/**
 * Click at CSS point (x, y) of the page, the way a pointer would: pointerdown/mousedown/pointerup/mouseup/click on
 * the topmost element there (the live mirror of an agent instance forwards a human's clicks through this).
 */
export function clickAt(x: number, y: number): Click {
	if (!Number.isFinite(x) || !Number.isFinite(y)) return { clicked: false, reason: "bad point" };
	const el = document.elementFromPoint(x, y) as HTMLElement | null;
	if (!el) return { clicked: false, reason: `nothing at ${x},${y}` };
	const at = { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, button: 0 };
	el.dispatchEvent(
		new PointerEvent("pointerdown", { ...at, buttons: 1, pointerType: "mouse", isPrimary: true }),
	);
	el.dispatchEvent(new MouseEvent("mousedown", { ...at, buttons: 1 }));
	if (el.matches("input, textarea, select, [contenteditable], button, a, [tabindex]")) el.focus();
	el.dispatchEvent(new PointerEvent("pointerup", { ...at, pointerType: "mouse", isPrimary: true }));
	el.dispatchEvent(new MouseEvent("mouseup", at));
	el.dispatchEvent(new MouseEvent("click", at));
	return { clicked: true, tag: el.tagName.toLowerCase(), text: (el.textContent ?? "").trim().slice(0, 80) };
}

type Pressed = { to: "terminal" | "field" | "page"; key: string };

/** A terminal's own key sink (GpuTerminal's hidden textarea): it turns the real keydown into terminal input. */
const terminalSink = (el: Element | null) => !!el?.matches('textarea[aria-label="Terminal input"]');

/**
 * One key from the live mirror. The focused terminal (no text field focused) gets the bytes a terminal sends;
 * otherwise the focused element gets keydown/keyup, and a printable key or Backspace edits a focused text field.
 * ⌘ shortcuts always go to the page, so the app's own bindings work from the mirror.
 */
export async function pressKey(k: Key): Promise<Pressed> {
	const el = document.activeElement as HTMLElement | null;
	if (terminalSink(el)) {
		// The terminal maps the keydown itself (Option-as-Alt, ⌘ terminal actions); a printable key it leaves to the
		// browser's default text insertion, which a synthetic event does not do: insert it the way the sink reads it.
		const init = {
			key: k.key,
			ctrlKey: !!k.ctrl,
			altKey: !!k.alt,
			metaKey: !!k.meta,
			shiftKey: !!k.shift,
			bubbles: true,
			cancelable: true,
			composed: true,
		};
		const go = el!.dispatchEvent(new KeyboardEvent("keydown", init));
		if (go && !k.ctrl && !k.meta && [...k.key].length === 1) {
			(el as HTMLTextAreaElement).value += k.key;
			el!.dispatchEvent(new InputEvent("input", { bubbles: true, data: k.key, inputType: "insertText" }));
		}
		el!.dispatchEvent(new KeyboardEvent("keyup", init));
		return { to: "terminal", key: k.key };
	}
	const field = el?.matches("input, textarea") ? (el as HTMLInputElement) : null;
	const editable = !!field || !!el?.isContentEditable;
	if (!editable) {
		const atlas = (window as unknown as { __atlas?: { ws?: any; host?: any } }).__atlas;
		const ws = atlas?.ws;
		const tab = ws?.selected ? ws.focusedTab(ws.selected.id) : undefined;
		const bytes = keyBytes(k);
		if (tab?.pty && bytes !== null) {
			await atlas!.host.ptyWrite(tab.pty, bytes);
			return { to: "terminal", key: k.key };
		}
	}
	const target = el ?? document.body;
	const init = {
		key: k.key,
		ctrlKey: !!k.ctrl,
		altKey: !!k.alt,
		metaKey: !!k.meta,
		shiftKey: !!k.shift,
		bubbles: true,
		cancelable: true,
		composed: true,
	};
	const go = target.dispatchEvent(new KeyboardEvent("keydown", init));
	if (go && field && !k.ctrl && !k.meta) {
		const [a, b] = [field.selectionStart ?? field.value.length, field.selectionEnd ?? field.value.length];
		const printable = [...k.key].length === 1;
		if (printable || (k.key === "Backspace" && (a > 0 || b > a))) {
			if (printable) field.setRangeText(k.key, a, b, "end");
			else field.setRangeText("", a === b ? a - 1 : a, b, "end");
			field.dispatchEvent(
				new InputEvent("input", {
					bubbles: true,
					data: printable ? k.key : null,
					inputType: printable ? "insertText" : "deleteContentBackward",
				}),
			);
		}
	}
	target.dispatchEvent(new KeyboardEvent("keyup", init));
	return { to: editable ? "field" : "page", key: k.key };
}

/** Types into the visible field at `selector`; returns the field's value afterwards. */
export const typeTarget = (selector: string, text: string, delayMs: number) =>
	typeInto(selector, text, delayMs);

export interface A11yNode {
	role: string;
	name?: string;
	problem?: string;
	children?: A11yNode[];
}

const IMPLICIT_ROLE: Record<string, string> = {
	main: "main",
	nav: "navigation",
	header: "banner",
	footer: "contentinfo",
	aside: "complementary",
	h1: "heading",
	h2: "heading",
	h3: "heading",
	h4: "heading",
	h5: "heading",
	h6: "heading",
	button: "button",
	a: "link",
	ul: "list",
	ol: "list",
	li: "listitem",
	input: "textbox",
	textarea: "textbox",
	select: "combobox",
	img: "img",
	table: "table",
	dialog: "dialog",
};

function nameOf(el: Element): string {
	return (
		el.getAttribute("aria-label") ??
		el.getAttribute("title") ??
		el.getAttribute("alt") ??
		(el.textContent ?? "").trim().replace(/\s+/g, " ").slice(0, 80)
	);
}

/**
 * The accessibility tree an assistive technology would walk: elements with a role (explicit or implicit), their
 * accessible name, nesting preserved; `aria-hidden` subtrees and role-less wrappers are dropped (their children are
 * hoisted). A control without a name carries a `problem`: the structural bug a screenshot cannot show.
 */
export function a11yTree(root: Element): A11yNode[] {
	const out: A11yNode[] = [];
	for (const el of root.children) {
		if (el.getAttribute("aria-hidden") === "true") continue;
		const role = el.getAttribute("role") ?? IMPLICIT_ROLE[el.tagName.toLowerCase()];
		const children = a11yTree(el);
		if (!role) {
			out.push(...children);
			continue;
		}
		const node: A11yNode = { role };
		const name = nameOf(el);
		if (name && role !== "list") node.name = name;
		if ((role === "button" || role === "link") && !name) node.problem = `${role} has no accessible name`;
		if (children.length) node.children = children;
		out.push(node);
	}
	return out;
}

export interface ShotRect {
	x: number;
	y: number;
	width: number;
	height: number;
	scale: number;
}

/** The element's rect in CSS px, clipped to the viewport, for a native element snapshot. */
export function elementShotRect(selector: string, view: { w: number; h: number; scale: number }): ShotRect {
	const r = target(selector, view).getBoundingClientRect();
	const x = Math.max(0, r.x);
	const y = Math.max(0, r.y);
	const width = Math.min(r.x + r.width, view.w) - x;
	const height = Math.min(r.y + r.height, view.h) - y;
	if (width <= 0 || height <= 0) throw new Error(`${selector} is outside the viewport`);
	return { x, y, width, height, scale: view.scale };
}

/** Longest side of a `--full` snapshot in CSS px: a runaway list must not exhaust memory. */
export const MAX_FULL_SIDE = 16384;

/**
 * The size of the whole scrollable content of `selector` (the document when omitted), in CSS px: what `--full`
 * needs the view to be so one snapshot shows everything, capped at {@link MAX_FULL_SIDE}.
 */
export function fullShotSize(selector: string | undefined, view: { w: number; h: number }) {
	const el = selector ? target(selector, { w: view.w, h: view.h }) : document.documentElement;
	const box = el.getBoundingClientRect();
	return {
		width: Math.min(MAX_FULL_SIDE, Math.ceil(Math.max(el.scrollWidth, selector ? box.width : view.w))),
		height: Math.min(MAX_FULL_SIDE, Math.ceil(Math.max(el.scrollHeight, selector ? box.height : view.h))),
	};
}

interface Stores {
	ws?: {
		loaded?: boolean;
		doc?: { worktrees?: { id: string; name: string }[]; selected?: string | null };
	};
	store?: {
		loaded?: boolean;
		projects?: { id: string; name: string; root: string }[];
		selected?: string | null;
		live?: Record<string, { services?: { name: string; state: string }[] }>;
	};
}

/** What the app is doing, compactly: projects with their services' states, worktrees, and the selection. */
export function stateSummary(s: Stores) {
	const selected: Record<string, string | undefined> = {};
	if (s.ws?.doc?.selected) selected.worktree = s.ws.doc.selected;
	if (s.store?.selected) selected.project = s.store.selected;
	return {
		loaded: { ws: !!s.ws?.loaded, store: !!s.store?.loaded },
		projects: (s.store?.projects ?? []).map((p) => ({
			id: p.id,
			name: p.name,
			root: p.root,
			services: Object.fromEntries((s.store?.live?.[p.id]?.services ?? []).map((x) => [x.name, x.state])),
		})),
		worktrees: (s.ws?.doc?.worktrees ?? []).map((w) => ({ id: w.id, name: w.name })),
		selected,
	};
}

/** Frame times and long tasks since `start()`: the numbers behind "it feels janky". */
export function perfRecorder() {
	const frames: number[] = [];
	const longTasks: { start: number; ms: number }[] = [];
	let raf = 0;
	let last = 0;
	let obs: PerformanceObserver | undefined;
	return {
		start() {
			last = performance.now();
			const tick = (t: number) => {
				frames.push(t - last);
				last = t;
				raf = requestAnimationFrame(tick);
			};
			raf = requestAnimationFrame(tick);
			try {
				obs = new PerformanceObserver((l) => {
					for (const e of l.getEntries())
						longTasks.push({ start: Math.round(e.startTime), ms: Math.round(e.duration) });
				});
				obs.observe({ entryTypes: ["longtask"] });
			} catch {
				// longtask entries are not available in every WebKit; frames are still measured
			}
		},
		stop() {
			cancelAnimationFrame(raf);
			obs?.disconnect();
			return perfReport(frames.slice(1), longTasks, document.visibilityState);
		},
	};
}

/** Pure: summarise frame gaps (ms). A page that never painted reports `fps: 0`, not NaN. */
export function perfReport(gaps: number[], longTasks: { start: number; ms: number }[], visibility: string) {
	const sorted = [...gaps].sort((a, b) => a - b);
	const pick = (p: number) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))] ?? 0;
	const total = gaps.reduce((a, b) => a + b, 0);
	return {
		visibility,
		frames: gaps.length,
		fps: total > 0 ? Math.round((gaps.length * 1000 * 10) / total) / 10 : 0,
		p50_ms: Math.round(pick(0.5)),
		p95_ms: Math.round(pick(0.95)),
		max_ms: Math.round(sorted.at(-1) ?? 0),
		long_tasks: longTasks,
	};
}
