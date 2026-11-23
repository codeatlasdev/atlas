import { pageIsFocused } from "#lib/onboarding/intro/player.ts";

/**
 * Whether the app's window has the keyboard focus, as reactive state: what a view that must not draw for a window
 * nobody is looking at (the health panel's charts, #289) reads. WebKitGTK on Wayland never reports a window behind
 * another one as hidden, so focus is the signal; an agent instance is never focused by design and counts as focused
 * (`pageIsFocused`), so its recordings and benches still see motion. One set of listeners for the app's lifetime.
 */
class WindowFocus {
	focused = $state(true);

	constructor() {
		if (typeof window === "undefined") return;
		const agent = "__ATLAS_AGENT__" in window;
		this.focused = pageIsFocused(document.hasFocus(), agent);
		window.addEventListener("focus", () => (this.focused = true));
		window.addEventListener("blur", () => (this.focused = pageIsFocused(false, "__ATLAS_AGENT__" in window)));
	}
}

export const windowFocus = new WindowFocus();
