import type { HandleClientError, ClientInit } from "@sveltejs/kit/hooks";
import { messageOf } from "#lib/errors/report.ts";
import { bootLocale } from "#lib/i18n/locale.svelte.ts";
import { blockFilePickers, openExternalLinks } from "#lib/nativeUi.ts";

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

/**
 * Before any page code runs: Atlas Lab (dev only) routes IPC of a plain browser tab to an agent instance, then the
 * display language is read, so the first render is already in it and nothing flashes English.
 */
export const init: ClientInit = async () => {
	openExternalLinks();
	if ("__ATLAS_AGENT__" in window && window.__ATLAS_AGENT__)
		blockFilePickers(document, (message) => console.error(message));
	if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
		const { installLabShim } = await import("#lib/debug/labShim.ts");
		await installLabShim();
	}
	await bootLocale();
};
