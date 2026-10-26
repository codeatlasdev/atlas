/**
 * Hover tooltips (kit `Tooltip`) close on `pointerleave`, which never fires when the window loses focus or is hidden
 * under a resting cursor (another app, a Space switch, minimize): the tooltip stayed on screen over whatever came next.
 * On window blur or a hidden document, every open tooltip trigger (`aria-describedby` + `data-state="open"`) gets the
 * `pointerleave` it missed. Menu and popover triggers carry no `aria-describedby` and are left alone.
 * Temporary until the kit does it itself (codeatlasdev/ui#663); delete this module then.
 */
export function dismissHoverOnBlur(): () => void {
	const dismiss = () => {
		for (const el of document.querySelectorAll('[aria-describedby][data-state="open"]'))
			el.dispatchEvent(new PointerEvent("pointerleave", { pointerType: "mouse" }));
	};
	const onVisibility = () => {
		if (document.visibilityState === "hidden") dismiss();
	};
	window.addEventListener("blur", dismiss);
	document.addEventListener("visibilitychange", onVisibility);
	return () => {
		window.removeEventListener("blur", dismiss);
		document.removeEventListener("visibilitychange", onVisibility);
	};
}
