// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { hostRestartPrompt } from "./hostPrompt.ts";
import { m } from "#lib/i18n/m.ts";

test("no terminals: safe, not destructive", () => {
	const p = hostRestartPrompt([]);
	expect(p.destructive).toBe(false);
	expect(p.description).toBe(m.workbench_host_prompt_no_terminals());
});

test("lists terminals by title and cwd and says they end", () => {
	const p = hostRestartPrompt([
		{ title: "zsh", cwd: "/Users/me/proj/app" },
		{ title: "bun dev", cwd: "/Users/me/proj" },
	]);
	expect(p.destructive).toBe(true);
	expect(p.description).toContain(m.workbench_host_prompt_ends({ count: 2 }));
	expect(p.description).toContain("• zsh · proj/app");
	expect(p.description).toContain("• bun dev · me/proj");
	expect(p.confirmLabel).toBe(m.workbench_host_prompt_end_and_restart({ count: 2 }));
});

test("a single terminal is singular", () => {
	const p = hostRestartPrompt([{ title: "", cwd: "/" }]);
	expect(p.description).toContain(m.workbench_host_prompt_ends({ count: 1 }));
	expect(p.description).toContain(`• ${m.crew_chat_header_terminal()} · /`);
	expect(p.confirmLabel).toBe(m.workbench_host_prompt_end_and_restart({ count: 1 }));
});

test("long lists are capped", () => {
	const ptys = Array.from({ length: 9 }, (_, i) => ({ title: `t${i}`, cwd: "/a/b" }));
	const p = hostRestartPrompt(ptys);
	expect(p.description).toContain(m.workbench_host_more_terminals({ count: 3 }));
	expect(p.description).not.toContain("t8");
});
