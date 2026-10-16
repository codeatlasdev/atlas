// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident (CI of the kit 4.0 upgrade): Tip.test.ts asked for a real `Tooltip` and replaced the whole package entry with
// `{ Tooltip }`. The next suite to import `toast` (workspace.test.ts) died on "Export named 'toast' not found" before its
// `afterAll`, so its stub `document` stayed and every DOM suite after it lost `document.body` (112 failures). Which suite
// ran first only depended on the file order of the runner, so it passed locally and failed on CI.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
test("asking for a real kit component keeps the stubs the other suites import", async () => {
	const { installUiKit, useRealKit, realKitComponent } = await import("../../test/uiKit.ts");
	await installUiKit();
	await useRealKit("Tooltip");
	const kit = await import("@codeatlasdev/ui");
	expect(kit.Tooltip).toBe(await realKitComponent("Tooltip"));
	expect(typeof kit.toast.error).toBe("function");
	expect(typeof kit.Button).toBe("function");
});

test("the order does not matter: installing the stubs after the real component keeps the real component", async () => {
	const { installUiKit, useRealKit, realKitComponent } = await import("../../test/uiKit.ts");
	await useRealKit("Tooltip");
	await installUiKit();
	const kit = await import("@codeatlasdev/ui");
	expect(kit.Tooltip).toBe(await realKitComponent("Tooltip"));
	expect(typeof kit.toast.error).toBe("function");
});
