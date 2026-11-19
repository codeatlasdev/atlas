// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { lspUiState, isServing, type UiInput } from "./status.ts";
import type { ServerStatus } from "./types.ts";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";

const base = (over: Partial<UiInput> = {}): UiInput => ({
	prefsEnabled: true,
	languageId: "typescript",
	host: { lsp: true },
	doc: { status: "ready", server: "vtsls" },
	srv: undefined,
	...over,
});

const srv = (over: Partial<ServerStatus> = {}): ServerStatus => ({
	name: "vtsls",
	label: "TypeScript",
	root: "/p",
	language: "typescript",
	state: "ready",
	...over,
});

const ids = (s) => s.actions.map((a) => a.id);

test("old host: chip, restart now/later, lists live terminals count", () => {
	const s = lspUiState(
		base({
			host: {
				lsp: false,
				ptys: [
					{ title: "zsh", cwd: "/p" },
					{ title: "bun dev", cwd: "/p/app" },
				],
			},
		}),
	);
	expect(s.kind).toBe("host-old");
	expect(s.label).toBe(m.workbench_lsp_host_old_label());
	expect(s.tone).toBe("warn");
	expect(ids(s)).toEqual(["restart-host", "restart-host-later"]);
	expect(s.ptys).toHaveLength(2);
	expect(s.detail).toContain("2 running terminals");
});

test("old host with no terminals says a restart is safe; one terminal is singular", () => {
	expect(lspUiState(base({ host: { lsp: false, ptys: [] } })).detail).toContain("safe");
	expect(lspUiState(base({ host: { lsp: false, ptys: [{ title: "a", cwd: "/" }] } })).detail).toContain(
		"1 running terminal.",
	);
});

test("host upgrading live (proto 2): busy, no actions, terminals are kept", () => {
	const s = lspUiState(base({ host: { lsp: false, updating: true, ptys: [{ title: "a", cwd: "/" }] } }));
	expect(s.kind).toBe("host-updating");
	expect(s.tone).toBe("busy");
	expect(s.label).toBe(m.workbench_lsp_host_updating_label());
	expect(s.actions).toEqual([]);
	expect(s.summary).toContain("keep running");
});

test("host gate wins over every other fact", () => {
	expect(lspUiState(base({ host: { lsp: false }, prefsEnabled: false })).kind).toBe("host-old");
});

test("setting off: enable action first", () => {
	const s = lspUiState(base({ prefsEnabled: false }));
	expect(s.kind).toBe("disabled-setting");
	expect(s.tone).toBe("idle");
	expect(s.actions[0]).toMatchObject({ id: "enable-setting", primary: true });
	expect(ids(s)).toContain("open-settings-languages");
});

test("language disabled by override", () => {
	const s = lspUiState(base({ languageDisabled: true }));
	expect(s.kind).toBe("disabled-language");
	expect(s.summary).toContain("typescript");
});

test("no language: carries the supported list", () => {
	const s = lspUiState(
		base({
			languageId: "brainfuck",
			doc: { status: "disabled", server: null, supported: ["Go", "Rust", "TypeScript"] },
		}),
	);
	expect(s.kind).toBe("no-language");
	expect(s.supported).toEqual(["Go", "Rust", "TypeScript"]);
	expect(s.detail).toBe(m.workbench_lsp_none_supported({ list: "Go, Rust, and TypeScript" }));
	expect(ids(s)).toEqual(["open-settings-languages", "docs"]);
});

test("no server and no failure in the reply is also no-language", () => {
	expect(lspUiState(base({ doc: { status: "starting", server: null } })).kind).toBe("no-language");
});

test("before the open reply (doc undefined) it is starting, never no-language", () => {
	const s = lspUiState(base({ doc: undefined }));
	expect(s.kind).toBe("starting");
	expect(s.tone).toBe("busy");
});

test("outside root", () => {
	expect(lspUiState(base({ outsideRoot: true, doc: undefined })).kind).toBe("outside-root");
	const s = lspUiState(
		base({
			doc: {
				status: "missing",
				server: "vtsls",
				reason: { code: "outside_root", message: "This file is outside the project." },
			},
		}),
	);
	expect(s.kind).toBe("outside-root");
	expect(s.summary).toBe("This file is outside the project.");
});

test("installing keeps the progress in the popover detail and offers cancel", () => {
	const s = lspUiState(base({ srv: srv({ state: "downloading", progress: "Downloading… 42%" }) }));
	expect(s.kind).toBe("installing");
	expect(s.tone).toBe("busy");
	expect(s.label).toBe(m.workbench_lsp_installing({ label: "TypeScript" }));
	// The progress text is in the popover, never in the chip.
	expect(s.progress).toBe("Downloading… 42%");
	expect(ids(s)).toEqual(["cancel-install", "open-log"]);
});

test("installing without progress text", () => {
	const s = lspUiState(base({ srv: srv({ state: "downloading" }) }));
	expect(s.label).toBe(m.workbench_lsp_installing({ label: "TypeScript" }));
	expect(s.detail).toBeUndefined();
});

test("starting, with progress as detail", () => {
	const s = lspUiState(base({ srv: srv({ state: "starting", progress: "Indexing 3/10" }) }));
	expect(s.kind).toBe("starting");
	expect(s.detail).toBe("Indexing 3/10");
	expect(isServing(s)).toBe(true);
});

test("ready: name, version and source", () => {
	const s = lspUiState(base({ srv: srv({ version: "0.2.1", source: "managed" }) }));
	expect(s.kind).toBe("ready");
	expect(s.tone).toBe("ok");
	expect(s.label).toBe("TypeScript 0.2.1");
	expect(s.detail).toContain("installed by Atlas");
	expect(isServing(s)).toBe(true);
	expect(lspUiState(base({ srv: srv({ source: "custom" }) })).detail).toContain("custom command");
	expect(lspUiState(base({ srv: srv({ source: "path" }) })).detail).toContain("PATH");
});

test("ready from the open reply alone, before the status push arrives", () => {
	const s = lspUiState(base({ doc: { status: "ready", server: "vtsls" } }));
	expect(s.kind).toBe("ready");
	expect(s.label).toBe("vtsls");
});

test("crashed: message, tail, restart is primary", () => {
	const s = lspUiState(
		base({
			srv: srv({ state: "crashed", message: "exited with code 1", tail: ["panic: boom", "at main.go:3"] }),
		}),
	);
	expect(s.kind).toBe("crashed");
	expect(s.tone).toBe("error");
	expect(s.summary).toBe("exited with code 1");
	expect(s.tail).toEqual(["panic: boom", "at main.go:3"]);
	expect(s.actions[0]).toMatchObject({ id: "restart-server", primary: true });
	expect(ids(s)).toContain("open-log");
});

test("crashed with no message or tail still explains", () => {
	const s = lspUiState(base({ srv: srv({ state: "crashed" }), doc: { status: "crashed", server: "vtsls" } }));
	expect(s.summary).toBe("TypeScript stopped unexpectedly.");
	expect(s.tail).toBeUndefined();
});

test("crashed from the open reply alone", () => {
	expect(lspUiState(base({ doc: { status: "crashed", server: "vtsls" } })).kind).toBe("crashed");
});

test("go missing: copyable brew command", () => {
	const s = lspUiState(
		base({
			languageId: "go",
			srv: srv({
				name: "gopls",
				label: "Go",
				state: "missing",
				reason: {
					code: "no_go",
					message: "Atlas needs Go to install gopls.",
					hint: "Install Go, then check again.",
					command: "brew install go",
				},
			}),
		}),
	);
	expect(s.kind).toBe("missing-toolchain");
	expect(s.command).toBe("brew install go");
	expect(s.label).toBe(m.workbench_lsp_needs_tool({ tool: "Go" }));
	expect(s.detail).toBe("Install Go, then check again.");
	expect(s.actions[0]).toEqual({ id: "copy-command", label: "Copy command", payload: "brew install go" });
	expect(ids(s)).toContain("retry");
});

test("curl missing without a command makes retry the primary action", () => {
	const s = lspUiState(
		base({
			doc: {
				status: "missing",
				server: "rust-analyzer",
				reason: { code: "no_curl", message: "curl is not available." },
			},
		}),
	);
	expect(s.kind).toBe("missing-toolchain");
	expect(s.actions[0]).toMatchObject({ id: "retry", primary: true });
	expect(ids(s)).not.toContain("copy-command");
});

test("offline offers retry install", () => {
	const s = lspUiState(
		base({
			srv: srv({ state: "missing", reason: { code: "offline", message: "You are offline." } }),
		}),
	);
	expect(s.kind).toBe("offline");
	expect(s.actions[0]).toMatchObject({ id: "install", primary: true });
});

test("install failed: retry, log; unsupported platform has no retry", () => {
	const failed = lspUiState(
		base({
			srv: srv({ state: "missing", reason: { code: "install_failed", message: "Download failed." } }),
		}),
	);
	expect(failed.kind).toBe("install-failed");
	expect(ids(failed)).toEqual(["install", "open-log", "open-settings-languages"]);
	const unsupported = lspUiState(
		base({
			srv: srv({
				state: "missing",
				reason: { code: "unsupported_platform", message: "No build for this platform." },
			}),
		}),
	);
	expect(unsupported.kind).toBe("install-failed");
	expect(ids(unsupported)).not.toContain("install");
});

test("custom command not found points at settings", () => {
	const s = lspUiState(
		base({
			srv: srv({
				state: "missing",
				reason: { code: "custom_not_found", message: "`my-ls` was not found." },
			}),
		}),
	);
	expect(s.kind).toBe("custom-not-found");
	expect(s.actions[0]).toMatchObject({ id: "open-settings-languages", primary: true });
});

test("missing with no reason offers install", () => {
	const s = lspUiState(base({ srv: srv({ state: "missing", message: "vtsls is not installed." }) }));
	expect(s.kind).toBe("missing");
	expect(s.summary).toBe("vtsls is not installed.");
	expect(s.actions[0]).toMatchObject({ id: "install" });
});

test("a crash whose reason is a toolchain problem reports the toolchain", () => {
	const s = lspUiState(
		base({
			srv: srv({ state: "crashed", reason: { code: "no_go", message: "Go is missing." } }),
		}),
	);
	expect(s.kind).toBe("missing-toolchain");
});

test("transitions: downloading -> starting -> ready -> crashed", () => {
	const kinds = (["downloading", "starting", "ready", "crashed"] as const).map(
		(state) => lspUiState(base({ srv: srv({ state }) })).kind,
	);
	expect(kinds).toEqual(["installing", "starting", "ready", "crashed"]);
});

test("transition: old host -> upgrading -> capable", () => {
	const old = lspUiState(base({ host: { lsp: false } }));
	const up = lspUiState(base({ host: { lsp: false, updating: true } }));
	const ok = lspUiState(base({ host: { lsp: true }, srv: srv() }));
	expect([old.kind, up.kind, ok.kind]).toEqual(["host-old", "host-updating", "ready"]);
});

// The chip is one short state word per state; the sentence lives in the popover. A chip as long as the sentence is cut
// by the status bar's max width and the state is the part that disappears (pt-BR "host too old", 980 px window).
test("every state's chip label is a short state word, in both languages, and never the popover's sentence", async () => {
	const states = (name: string) => [
		lspUiState(base({ host: { lsp: false } })),
		lspUiState(base({ host: { lsp: false, updating: true } })),
		lspUiState(base({ prefsEnabled: false })),
		lspUiState(base({ languageDisabled: true })),
		lspUiState(base({ outsideRoot: true, doc: undefined })),
		lspUiState(base({ doc: { status: "disabled", server: null } })),
		lspUiState(base({ srv: srv({ name, label: "", state: "downloading", progress: "Downloading… 42%" }) })),
		lspUiState(base({ srv: srv({ name, label: "", state: "crashed" }) })),
		lspUiState(base({ srv: srv({ name, label: "", state: "missing" }) })),
		lspUiState(
			base({ srv: srv({ name, label: "", state: "missing", reason: { code: "no_go", message: "x" } }) }),
		),
		lspUiState(
			base({ srv: srv({ name, label: "", state: "missing", reason: { code: "offline", message: "x" } }) }),
		),
		lspUiState(
			base({
				srv: srv({ name, label: "", state: "missing", reason: { code: "install_failed", message: "x" } }),
			}),
		),
		lspUiState(base({ srv: srv({ name, label: "", state: "starting" }) })),
	];
	for (const tag of ["en", "pt-BR"] as const) {
		await inLocale(tag, () => {
			for (const s of states("ts")) {
				expect(s.label.length, `${tag} ${s.kind}: ${s.label}`).toBeLessThanOrEqual(28);
				expect(s.label, `${tag} ${s.kind}`).not.toBe(s.summary);
			}
		});
	}
});
