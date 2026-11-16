/**
 * Whether `e` is rendered: it has a layout box (not `display: none`, itself or an ancestor) and is not
 * `visibility: hidden`. The one answer the agent bridge, scripted captures and the contrast check use.
 *
 * Not `offsetParent !== null`: that is null for `<body>`, `<html>` and every `position: fixed` element (dialogs,
 * sheets, toasts, the dock), so `atlas desktop debug wait body` never resolved and a fixed overlay could not be
 * clicked or shot.
 */
export function isRendered(e: Element): boolean {
	if (typeof e.checkVisibility === "function") return e.checkVisibility({ visibilityProperty: true });
	return e.getClientRects().length > 0;
}
