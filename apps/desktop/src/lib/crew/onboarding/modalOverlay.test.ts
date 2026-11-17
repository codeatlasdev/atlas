// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, describe, expect, test } from "bun:test";
import { modalOverlay } from "./modalOverlay.ts";

// #517 (maintainer's screenshots): "Board ⇧⌘C" and "Workspaces ⇧⌘B", the shell's own chrome under the onboarding,
// showed over it. Browsers do not hover, focus or open a Tip inside an `inert` subtree, so the contract is: the
// overlay is a direct child of <body>, everything else is inert while it lives, and nothing else changes after it.
afterEach(() => (document.body.innerHTML = ""));

function app() {
	document.body.innerHTML = `<div id="app"><div id="shell"><button id="tip-trigger">Board</button><div id="host"></div></div></div><div id="toasts" inert></div>`;
	const overlay = document.createElement("div");
	overlay.id = "overlay";
	document.querySelector("#host").append(overlay);
	return overlay;
}

describe("a modal overlay makes the app behind it inert", () => {
	test("the overlay moves out of the app and the app (a Tip's trigger included) is inert", () => {
		const overlay = app();
		const action = modalOverlay(overlay);
		expect(overlay.parentElement).toBe(document.body);
		expect(overlay.hasAttribute("inert")).toBe(false);
		expect(document.querySelector("#app").hasAttribute("inert")).toBe(true);
		// The trigger of a Tip under the overlay sits inside an inert subtree: it cannot be hovered or focused.
		expect(document.querySelector("#tip-trigger").closest("[inert]")).not.toBeNull();
		action.destroy();
	});

	test("closing it restores the app, puts the node back, and keeps what was inert before inert", () => {
		const overlay = app();
		const action = modalOverlay(overlay);
		action.destroy();
		expect(document.querySelector("#app").hasAttribute("inert")).toBe(false);
		expect(document.querySelector("#toasts").hasAttribute("inert")).toBe(true);
		expect(overlay.parentElement.id).toBe("host");
	});
});
