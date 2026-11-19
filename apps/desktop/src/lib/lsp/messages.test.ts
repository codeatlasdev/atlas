// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { feedbackFor, firstTime, hintAction, reasonForFeature, resetHints } from "./messages.ts";
import { lspUiState, type UiInput } from "./status.ts";
import { m } from "#lib/i18n/m.ts";

const input = (over: Partial<UiInput> = {}): UiInput => ({
	prefsEnabled: true,
	languageId: "typescript",
	host: { lsp: true },
	doc: { status: "ready", server: "vtsls" },
	srv: undefined,
	...over,
});

test("old host: names the feature and the fix", () => {
	const s = lspUiState(input({ host: { lsp: false } }));
	expect(reasonForFeature("definition", s)).toBe(
		`${m.workbench_lsp_needs_definition()} ${m.workbench_lsp_why_host_old()}`,
	);
});

test("nothing to explain when the server is ready", () => {
	expect(reasonForFeature("rename", lspUiState(input()))).toBeUndefined();
});

test("each feature has its own opening", () => {
	const s = lspUiState(input({ prefsEnabled: false }));
	expect(reasonForFeature("references", s)).toStartWith("Find references needs a language server.");
	expect(reasonForFeature("code-actions", s)).toStartWith("Code actions need a language server.");
	expect(reasonForFeature("format", s)).toContain("Language servers are turned off in settings.");
});

test("installing carries the progress", () => {
	const s = lspUiState(
		input({
			srv: { name: "gopls", root: "/p", language: "go", state: "downloading", progress: "42%" },
		}),
	);
	expect(reasonForFeature("definition", s)).toContain("installing (42%)");
});

test("toolchain problems reuse the state's own sentence", () => {
	const s = lspUiState(
		input({
			doc: {
				status: "missing",
				server: "gopls",
				reason: { code: "no_go", message: "Atlas needs Go to install gopls." },
			},
		}),
	);
	expect(reasonForFeature("definition", s)).toBe(
		"Go to definition needs a language server. Atlas needs Go to install gopls.",
	);
});

test("never the bare, unexplained message", () => {
	for (const over of [
		{ host: { lsp: false } },
		{ prefsEnabled: false },
		{ doc: { status: "disabled", server: null, supported: ["Go"] } },
		{ doc: { status: "crashed", server: "x" } },
		{ doc: undefined },
	]) {
		const msg = reasonForFeature("definition", lspUiState(input(over)));
		expect(msg).toBeDefined();
		expect(msg).not.toBe("No language server");
		expect(msg.split(". ").length).toBeGreaterThan(1);
	}
});

test("hint action is the primary action of the state", () => {
	expect(hintAction(lspUiState(input({ host: { lsp: false } })))).toEqual({
		id: "restart-host",
		label: "Restart terminal host",
	});
	expect(hintAction(lspUiState(input({ prefsEnabled: false })))).toEqual({
		id: "enable-setting",
		label: "Turn on",
	});
});

test("firstTime: once per document and feature, then silent", () => {
	const a = {};
	const b = {};
	expect(firstTime(a, "definition")).toBe(true);
	expect(firstTime(a, "definition")).toBe(false);
	expect(firstTime(a, "rename")).toBe(true);
	expect(firstTime(b, "definition")).toBe(true);
});

test("resetHints makes the explanation news again", () => {
	const a = {};
	firstTime(a, "format");
	resetHints(a);
	expect(firstTime(a, "format")).toBe(true);
});

// Defect: "F12 / ⌘-click silently does nothing". The hint used to be once per document+feature, so
// the second press (the user retrying after the 8 s note faded) was a no-op with no feedback.
test("every press of a navigation shortcut gets feedback; only a held key is collapsed", () => {
	const doc = {};
	const t0 = 1_000;
	expect(feedbackFor(doc, "definition", t0)).toBe("show");
	// A held key (auto-repeat within the debounce) does not stack up messages.
	expect(feedbackFor(doc, "definition", t0 + 50)).toBe("hold");
	// A deliberate second press later is answered again, never silent.
	expect(feedbackFor(doc, "definition", t0 + 2_000)).toBe("show");
	expect(feedbackFor(doc, "definition", t0 + 9_000)).toBe("show");
});

test("feedback is per document and per feature", () => {
	const a = {};
	const b = {};
	expect(feedbackFor(a, "definition", 0)).toBe("show");
	expect(feedbackFor(b, "definition", 10)).toBe("show");
	expect(feedbackFor(a, "references", 10)).toBe("show");
});
