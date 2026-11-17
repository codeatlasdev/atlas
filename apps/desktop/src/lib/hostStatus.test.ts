// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { chipFor, EMPTY_HOST, parseHostState, promptCopy, shouldPrompt } from "./hostStatus.ts";

const REASON =
	"Terminal host 0.6.2 is older than Atlas 1.2.1; language features and fast terminal rendering are off until it restarts.";

const outdated = (over = {}) => ({
	...EMPTY_HOST,
	status: "outdated",
	skew: "needsRestart",
	version: "0.6.2",
	proto: 1,
	reason: REASON,
	missing: ["pty_attach_raw", "lsp"],
	ptys: [
		{ id: "a", title: "bun run dev", cwd: "/Users/me/Projetos/atlas", argv: ["zsh"] },
		{ id: "b", title: "", cwd: "/tmp/x", argv: ["cargo", "watch"] },
		{ id: "c", title: " ", cwd: "", argv: [] },
	],
	...over,
});

describe("parseHostState", () => {
	test("reads the camelCase payload from Rust", () => {
		const s = parseHostState({ ...outdated(), appVersion: "1.2.1", deferred: true, idleArmed: true });
		expect(s.status).toBe("outdated");
		expect(s.appVersion).toBe("1.2.1");
		expect(s.deferred && s.idleArmed).toBe(true);
		expect(s.ptys).toHaveLength(3);
	});
	test("is defensive: junk fields fall back, bad ptys are dropped", () => {
		const s = parseHostState({
			status: "weird",
			skew: 4,
			version: 1,
			caps: ["lsp", 2, null],
			ptys: [{ id: "ok", title: 3 }, { title: "no id" }, null, "x"],
		});
		expect(s).toEqual({
			...EMPTY_HOST,
			caps: ["lsp"],
			ptys: [{ id: "ok", title: "", cwd: "", argv: [] }],
		});
	});
	test("anything that is not an object is no state", () => {
		for (const v of [null, undefined, "x", 3, []]) expect(parseHostState(v)).toBeNull();
	});
});

describe("chipFor", () => {
	test("a current host has no chip", () => {
		expect(chipFor(EMPTY_HOST)).toBeNull();
	});
	test("a host that needs the user is amber and carries the reason", () => {
		expect(chipFor(outdated())).toEqual({
			tone: "warn",
			label: "Terminal host needs a restart",
			detail: REASON,
		});
	});
	test("armed 'when idle' says so instead of asking", () => {
		expect(chipFor(outdated({ idleArmed: true })).label).toBe("Terminal host restarts when idle");
	});
	test("'Later' keeps the chip", () => {
		expect(chipFor(outdated({ deferred: true })).tone).toBe("warn");
	});
	test("only the version differs: calm and neutral", () => {
		const c = chipFor({
			...EMPTY_HOST,
			version: "1.1.0",
			skew: "needsRestart",
			reason: "Terminal host 1.1.0 is older than Atlas 1.2.0.",
		});
		expect(c.tone).toBe("info");
		expect(c.label).toBe("Terminal host 1.1.0");
	});
	test("an upgrade or restart under way is a quiet busy chip", () => {
		expect(chipFor({ ...EMPTY_HOST, status: "upgrading", skew: "upgradable" }).tone).toBe("busy");
		expect(chipFor({ ...EMPTY_HOST, status: "restarting" }).label).toBe("Restarting terminal host…");
	});
});

describe("promptCopy", () => {
	test("lists every running terminal as <title|command> — <~/cwd>", () => {
		const c = promptCopy(outdated());
		expect(c.title).toBe("Restart the terminal host?");
		expect(c.consequence).toBe("Restarting ends these terminals.");
		expect(c.body).toBe(REASON);
		expect(c.items).toEqual(["bun run dev — ~/Projetos/atlas", "cargo watch — /tmp/x", "shell — ~"]);
	});
});

describe("shouldPrompt", () => {
	test("asks for a blocked host with running terminals", () => {
		expect(shouldPrompt(outdated())).toBe(true);
	});
	test("not after 'Later', not when armed, not without terminals, not when current", () => {
		expect(shouldPrompt(outdated({ deferred: true }))).toBe(false);
		expect(shouldPrompt(outdated({ idleArmed: true }))).toBe(false);
		expect(shouldPrompt(outdated({ ptys: [] }))).toBe(false);
		expect(shouldPrompt(EMPTY_HOST)).toBe(false);
	});
});
