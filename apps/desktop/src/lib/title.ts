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
