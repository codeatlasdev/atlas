/**
 * A click on a native view (GPU terminal, browser page, any future NSView above the page) never reaches the DOM, so
 * everything that closes on an outside press (kit menus, selects, popovers, the palette) stayed open until the user
 * clicked the page. `natives.rs` reports every such mouse-down as `native://pointerdown` (window points); the page
 * delivers the `pointerdown` + `mousedown` the overlays listen for, at the element under that point (the native view's
 * DOM slot). Nothing else happens: the native view still handles the click itself.
 */
import { listen } from "@tauri-apps/api/event";
import { appliedZoom } from "#lib/zoom.ts";

type Rect = { x: number; y: number; w: number; h: number };

/** Re-deliver one native press. `toCss` maps window points to CSS px (layout zoom). */
export function deliverNativeClick(p: { x: number; y: number }, toCss: (r: Rect) => Rect): void {
	const { x, y } = toCss({ x: p.x, y: p.y, w: 0, h: 0 });
	const target = document.elementFromPoint(x, y) ?? document.body;
	const init = { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, button: 0 };
	target.dispatchEvent(new PointerEvent("pointerdown", { ...init, pointerType: "mouse", isPrimary: true }));
	target.dispatchEvent(new MouseEvent("mousedown", init));
}

/** Listen for native presses for the life of the app; returns the unlisten. */
export function startNativeClicks(): () => void {
	const off = listen<{ x: number; y: number }>("native://pointerdown", (e) =>
		deliverNativeClick(e.payload, (r) => {
			const z = appliedZoom() || 1;
			return { x: r.x / z, y: r.y / z, w: r.w / z, h: r.h / z };
		}),
	);
	return () => void off.then((f) => f());
}
