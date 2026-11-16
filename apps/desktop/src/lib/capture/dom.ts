/**
 * DOM-level actions of a scripted recording: synthetic events inside the webview, never OS input, so the user's
 * keyboard, mouse and focus are untouched. Timer-driven on purpose: WebKit throttles requestAnimationFrame for
 * windows that are not frontmost, and a recording of an occluded window must still advance.
 */

import { isRendered } from "../dom/visible.ts";
import { parseCombo } from "./motion.ts";
import type { Rect } from "./timeline.ts";

export type Sleep = (ms: number) => Promise<void>;
export const realSleep: Sleep = (ms) => {
	const { promise, resolve } = Promise.withResolvers<void>();
	setTimeout(resolve, ms);
	return promise;
};

const visible = isRendered;

export function elementRect(selector: string): Rect | undefined {
	const el = document.querySelector(selector);
	if (!el) return undefined;
	const r = el.getBoundingClientRect();
	return { x: r.x, y: r.y, width: r.width, height: r.height };
}

export function clickBySelector(selector: string): boolean {
	const el = [...document.querySelectorAll<HTMLElement>(selector)].find(visible);
	el?.click();
	return !!el;
}

const TEXT_TAGS = "button,[role=menuitem],[role=option],a,div,span";

/** The first visible element whose whole trimmed text is `text`. */
export function findByText(text: string, tags = TEXT_TAGS): HTMLElement | undefined {
	return [...document.querySelectorAll<HTMLElement>(tags)].find(
		(e) => e.textContent?.trim() === text && visible(e),
	);
}

export function clickByText(text: string): boolean {
	const el = findByText(text);
	el?.click();
	return !!el;
}

/** Type `text` into the input/textarea at `selector`, one character per `delayMs`, firing the events a keyboard does. */
export async function typeInto(
	selector: string,
	text: string,
	delayMs: number,
	sleep: Sleep = realSleep,
): Promise<string> {
	const el = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector);
	if (!el) throw new Error(`type: no element matches ${selector}`);
	if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement))
		throw new Error(`type: ${selector} is not an input or textarea`);
	const proto = el instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
	// the native setter, so frameworks that track `value` (Svelte bind:value) see the change
	const set = Object.getOwnPropertyDescriptor(proto, "value")!.set!;
	el.focus();
	for (const ch of text) {
		set.call(el, el.value + ch);
		el.dispatchEvent(new InputEvent("input", { bubbles: true, data: ch, inputType: "insertText" }));
		await sleep(delayMs);
	}
	return el.value;
}

/** Dispatch a key combo to the focused element (or the body). True when something called preventDefault. */
export function pressKey(combo: string): boolean {
	const c = parseCombo(combo);
	const ev = new KeyboardEvent("keydown", {
		key: c.key,
		code: c.code,
		metaKey: c.meta,
		shiftKey: c.shift,
		ctrlKey: c.ctrl,
		altKey: c.alt,
		bubbles: true,
		cancelable: true,
	});
	(document.activeElement ?? document.body).dispatchEvent(ev);
	return ev.defaultPrevented;
}

/** Eased scroll of the element at `selector` to `top` over `ms` (setInterval-driven, see the file header). */
export function smoothScroll(
	selector: string,
	top: number,
	ms: number,
	ease: (t: number) => number,
): Promise<number> {
	const el = document.querySelector<HTMLElement>(selector);
	if (!el) return Promise.reject(new Error(`scroll: no element matches ${selector}`));
	const from = el.scrollTop;
	const t0 = Date.now();
	const { promise, resolve } = Promise.withResolvers<number>();
	const id = setInterval(() => {
		const t = ms <= 0 ? 1 : Math.min(1, (Date.now() - t0) / ms);
		el.scrollTop = from + (top - from) * ease(t);
		if (t >= 1) {
			clearInterval(id);
			resolve(el.scrollTop);
		}
	}, 16);
	return promise;
}

export interface Clock {
	now(): number;
	sleep: Sleep;
}
const realClock: Clock = { now: Date.now, sleep: realSleep };

/**
 * Resolves (with the time it took) once `read()` has returned the same non-empty text for `quietMs`. A shell is
 * ready for typing when its startup output stops, not after a fixed sleep: fish alone can hold its prompt for 10 s
 * waiting for a terminal-capability reply. Rejects on timeout, saying whether anything was ever printed.
 */
export async function waitForQuiet(
	read: () => Promise<string>,
	o: { quietMs: number; timeoutMs: number; pollMs: number },
	clock: Clock = realClock,
): Promise<number> {
	const t0 = clock.now();
	let last = "";
	let changedAt = t0;
	for (;;) {
		const text = await read();
		const now = clock.now();
		if (text !== last) {
			last = text;
			changedAt = now;
		} else if (text !== "" && now - changedAt >= o.quietMs) {
			return now - t0;
		}
		if (now - t0 >= o.timeoutMs)
			throw new Error(
				last === ""
					? `no output after ${o.timeoutMs} ms`
					: `output was still changing after ${o.timeoutMs} ms`,
			);
		await clock.sleep(o.pollMs);
	}
}
