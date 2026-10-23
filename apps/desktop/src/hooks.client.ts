import type { HandleClientError, ClientInit } from "@sveltejs/kit/hooks";
import { messageOf } from "#lib/errors/report.ts";

/**
 * SvelteKit reduces anything unexpected to `{ status: 500, message: "Internal Error" }` so a
 * server never leaks internals. This is a local app with no remote audience, and a report that
 * says "Internal Error" is useless: keep the real message for `+error.svelte` (the console
 * still gets the original error with its stack).
 */
export const handleError: HandleClientError = ({ error, kind }) => {
	if (kind === "unknown") {
		console.error("[atlas] unhandled route error:", error);
		return { message: messageOf(error), stack: error instanceof Error ? error.stack : undefined };
	}
};

/** Atlas Lab (dev only): in a plain browser tab, route IPC to an agent instance before any page code runs. */
export const init: ClientInit = async () => {
	if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
		const { installLabShim } = await import("#lib/debug/labShim.ts");
		await installLabShim();
	}
};
