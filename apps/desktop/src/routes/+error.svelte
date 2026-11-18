<script lang="ts">
	/**
	 * Replaces SvelteKit's bare "500 Internal Error" page, for the main window and the tray alike.
	 * It never shows the status text: only what happened and what to do.
	 */
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../app.css";
	import { page } from "$app/state";
	import { onMount } from "svelte";
	import { m } from "#lib/i18n/m.ts";
	import { errors } from "#lib/errors/errors.svelte.ts";
	import ErrorPanel from "#lib/errors/ErrorPanel.svelte";
	import { report, type ErrorReport } from "#lib/errors/report.ts";

	const notFound = $derived(page.status === 404);
	const where = $derived(`route ${page.url.pathname}`);
	let version = $state<string | undefined>();
	let enriched = $state<ErrorReport | undefined>();
	onMount(() => void errors.version().then((v) => (version = v)));

	// Unexpected errors arrive with their real message and stack (`hooks.client.ts`); SvelteKit's own
	// statuses ("Not Found") are not something to show, so say what happened in plain words.
	const base = $derived<ErrorReport>({
		...report(
			notFound
				? { message: m.errors_route_no_screen_at({ path: page.url.pathname }) }
				: (page.error ?? "Unknown error"),
			{ where },
		),
		version,
	});
	// Machine facts and what the app did just before: added when ready, the screen never waits for them.
	$effect(() => {
		const b = base;
		void errors.enrich(b).then((e) => (enriched = e));
	});
	const r = $derived(enriched && enriched.message === base.message ? enriched : base);
</script>

<ErrorPanel
	report={r}
	variant="fullscreen"
	title={notFound ? m.errors_route_not_found_title() : m.errors_route_title()}
	hint={notFound ? m.errors_route_not_found_hint() : m.errors_route_hint()}
	retry={() => location.replace("/")}
	retryLabel={m.errors_route_reload()}
/>
