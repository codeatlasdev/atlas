// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, mock, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// Kit 4.2.0 (ui#194): `DropdownMenuItem` and `TabsTrigger` take an `icon` snippet (fixed 14px box, `aria-hidden`, `brandIcon`
// keeps a brand mark's own colors) and `SchemeAsset` picks the light/dark mark with CSS from the kit's `.dark`. Atlas used to
// hand-roll all three: a sized span in the item's children, and a read of its own theme store for the dark variant of the
// Codex blossom. Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
// The pre-4.2 component read this store for the dark variant; Vite-only `import.meta.glob` in it cannot load under bun. A stub
// keeps the old component loadable so the assertions below, not an import crash, are what fail against it.
mock.module(join(import.meta.dir, "../stores/theme.svelte.ts"), () => ({
	themes: { current: { appearance: "light" } },
}));
const { flushSync, mount, unmount, createRawSnippet } = await import("svelte");
const { realKitModule, useRealKit } = await import("../../../test/uiKit.ts");
// `SchemeAsset` is one of the shared stubs (test/uiKit.ts), so the entry another suite replaced still links; this suite
// then swaps in the real one before `AgentLogo` loads, because it asserts what the kit renders.
await useRealKit("SchemeAsset");
const { default: AgentLogo } = await import("./AgentLogo.svelte");

function render(props) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(AgentLogo, { target, props }));
	return { target, done: () => (unmount(app), target.remove()) };
}

test("a monochrome mark (Codex) renders BOTH variants and lets the kit's CSS pick one by scheme", () => {
	const { target, done } = render({ agent: "codex" });
	const light = target.querySelector(".ca-scheme-asset__light svg");
	const dark = target.querySelector(".ca-scheme-asset__dark svg");
	expect(light).not.toBeNull();
	expect(dark).not.toBeNull();
	expect(light.outerHTML).not.toBe(dark.outerHTML);
	done();
});

test("a colored mark (Claude, omp) reads on both schemes: the same mark in both variants", () => {
	for (const agent of ["claude", "omp"]) {
		const { target, done } = render({ agent });
		const light = target.querySelector(".ca-scheme-asset__light svg");
		const dark = target.querySelector(".ca-scheme-asset__dark svg");
		expect(light).not.toBeNull();
		expect(dark?.outerHTML).toBe(light?.outerHTML);
		done();
	}
});

test("the mark is decorative and sized by the caller; an unknown agent falls back to the generic icon, not a blank", () => {
	const { target, done } = render({ agent: "claude", size: 13 });
	expect(target.querySelector("[aria-hidden=true]")).not.toBeNull();
	expect(target.querySelector("[style*='13px']")).not.toBeNull();
	done();
	const fallback = render({ agent: "not-an-agent" });
	expect(fallback.target.querySelector(".ca-scheme-asset")).toBeNull();
	expect(fallback.target.querySelector("svg")).not.toBeNull();
	fallback.done();
});

test("AgentLogo no longer reads the app theme store: the kit's scheme decides (SSR-safe, no flash)", () => {
	const src = readFileSync(join(import.meta.dir, "AgentLogo.svelte"), "utf8");
	expect(src).not.toMatch(/stores\/theme/);
	expect(src).not.toMatch(/themes\./);
});

test("the kit item draws the icon slot in its fixed aria-hidden box and keeps a brand mark's colors", async () => {
	const Item = (await realKitModule("components/DropdownMenu/DropdownMenuItem.svelte")).default;
	const target = document.createElement("div");
	document.body.append(target);
	const icon = createRawSnippet(() => ({ render: () => `<svg id="mark"></svg>` }));
	const children = createRawSnippet(() => ({ render: () => `<span>New Codex session</span>` }));
	const app = flushSync(() => mount(Item, { target, props: { icon, brandIcon: true, children } }));
	const box = target.querySelector(".ca-dropdown-item__icon");
	expect(box?.getAttribute("aria-hidden")).toBe("true");
	expect(box?.hasAttribute("data-brand")).toBe(true);
	expect(box?.querySelector("#mark")).not.toBeNull();
	unmount(app);
	target.remove();
});

test("in the kit's icon slot the logo is bare: the slot draws the box, so there is no second sized wrapper", () => {
	const { target, done } = render({ agent: "claude", bare: true });
	expect(target.querySelector(".logo")).toBeNull();
	expect(target.querySelector(".ca-scheme-asset__light svg")).not.toBeNull();
	done();
	const fallback = render({ agent: "nope", bare: true });
	expect(fallback.target.querySelector("svg")?.getAttribute("width")).not.toBe("14");
	fallback.done();
});

// Incident (4.2 bump, seen in an agent instance): the kit sizes only a DIRECT svg child of the icon box, but SchemeAsset
// wraps the mark in display:contents spans, so Claude Code and omp (viewBox, no width/height) rendered 0x0 and Codex
// (width="24") overflowed the 14px box. codeatlasdev/ui#297. A bare mark must size itself to its box.
test("a bare logo's svg fills its box for every agent, light and dark", () => {
	for (const agent of ["omp", "claude", "codex"]) {
		const { target, done } = render({ agent, bare: true });
		const svgs = [...target.querySelectorAll(".ca-scheme-asset svg")].filter(
			(s) => !s.parentElement?.closest("svg"),
		);
		expect(svgs.length, agent).toBeGreaterThan(0);
		for (const s of svgs) {
			expect([s.getAttribute("width"), s.getAttribute("height")], agent).toEqual(["100%", "100%"]);
		}
		done();
	}
});
