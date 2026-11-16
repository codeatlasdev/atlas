import type { AgentState } from "#lib/stores/agents.svelte.ts";

/**
 * Agent state from a terminal title (fallback when no hook has arrived, e.g.
 * after an app restart). omp titles look like `π : task` (working),
 * `π ! task` (needs approval), `π > task` (idle); a braille spinner also means working.
 */
export function titleState(title: string): AgentState | null {
	const m = title.match(/(?:^|[\s|])(π|OMP)[ \t]+([:!>])/);
	if (m) return m[2] === ":" ? "working" : m[2] === "!" ? "blocked" : "idle";
	return /[\u2801-\u28ff]/.test(title) ? "working" : null;
}

/**
 * What a terminal tab shows for the title its shell set (OSC 0/2). Shells put `user@host:dir` there; with no user
 * name (an empty `$USER`, as in a container or an agent instance) that is `@host:dir`, a tab named "@oma:~/Projects/x"
 * (1.5.0-alpha.26). A leading `@` with nothing before it is dropped when it begins a `host`, `host:dir` shape; anything
 * else (`@angular/cli`, `user@host:dir`, `host:dir`, `π : task`) is left as the program wrote it.
 */
export function tabTitle(raw: string): string {
	return raw.trim().replace(/^@(?=[\w.-]+(?::|$))/, "");
}
