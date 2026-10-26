/**
 * Workaround for a kit Tooltip bug (codeatlasdev/ui): it closes on the trigger's `pointerleave`, `blur` and Escape
 * only. Cmd-Tab, a space switch or a hidden window while the pointer rests on a control delivers none of them, so the
 * tooltip stays open over whatever is shown next. Every trigger registers here; when the window loses focus or the
 * page is hidden the guard sends each one the `pointerleave` + `blur` the kit already handles.
 *
 * Delete this file and its callers once the kit closes on window blur / `visibilitychange` itself.
 */

import { createAttachmentKey } from "svelte/attachments";

const triggers = new Set<HTMLElement>();
let armed = false;

function closeAll() {
	for (const el of triggers) {
		el.dispatchEvent(new PointerEvent("pointerleave"));
		el.dispatchEvent(new FocusEvent("blur"));
	}
}

function onVisibility() {
	if (document.visibilityState === "hidden") closeAll();
}

function arm(on: boolean) {
	if (on === armed) return;
	armed = on;
	const f = on ? addEventListener : removeEventListener;
	f.call(window, "blur", closeAll);
	(on ? document.addEventListener : document.removeEventListener).call(
		document,
		"visibilitychange",
		onVisibility,
	);
}

/** Svelte attachment: track a tooltip trigger for as long as it is mounted. */
export function guardTooltip(node: HTMLElement) {
	triggers.add(node);
	arm(true);
	return () => {
		triggers.delete(node);
		if (!triggers.size) arm(false);
	};
}

const guardKey = createAttachmentKey();

/** The kit's trigger props plus the guard as a second attachment (its key is a fresh symbol, so it cannot replace the kit's). */
export const guarded = (props: Record<string, unknown>) => ({ ...props, [guardKey]: guardTooltip });

export { closeAll };
export const tracked = () => triggers.size;
