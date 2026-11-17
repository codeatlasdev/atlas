<script lang="ts">
	/** Settings ▸ Updates ▸ Terminal host tile: version, why it is behind, running terminals, choices. */
	import { Button, Spinner } from "@codeatlasdev/ui";
	import { SquareTerminal } from "@lucide/svelte";
	import { m } from "#lib/i18n/m.ts";
	import {
		hostState as h,
		openHostPrompt,
		postponeHost,
		restartHost,
	} from "#lib/stores/hostStatus.svelte.ts";
	import Tile from "./Tile.svelte";

	let busy = $derived(h.status === "upgrading" || h.status === "restarting");
	let waiting = $derived(h.status === "outdated");
	/** One line: the full sentence (`h.reason`, written by Rust in English) is on the tile's tooltip and in the prompt. */
	let sub = $derived.by(() => {
		if (busy)
			return h.status === "upgrading" ? m.settings_host_sub_updating() : m.settings_host_sub_restarting();
		const count = h.ptys.length;
		if (!h.reason) return m.settings_host_sub_current({ count });
		return m.settings_host_sub_older({ version: h.appVersion, count });
	});
</script>

<Tile
	icon={SquareTerminal}
	name={m.settings_host_name()}
	version={h.version ?? "—"}
	{sub}
	tone={h.reason ? "warn" : "neutral"}
	title={h.reason || undefined}
>
	{#snippet badges()}
		{#if h.idleArmed}<span class="tag info">{m.settings_host_badge_restarts_when_idle()}</span
			>{:else if h.deferred}<span class="tag warn">{m.settings_host_badge_postponed()}</span>{/if}
		{#if busy}<Spinner size={12} />{/if}
	{/snippet}
	{#snippet action()}
		{#if waiting && h.idleArmed}
			<Button size="sm" variant="ghost" onclick={postponeHost}>{m.settings_host_action_dont_wait()}</Button>
		{:else if waiting}
			<Button size="sm" variant="outlined" onclick={openHostPrompt}
				>{m.settings_host_action_restart_confirm()}</Button
			>
		{:else if h.skew === "needsRestart" && !busy}
			<Button size="sm" variant="outlined" onclick={h.ptys.length ? openHostPrompt : restartHost}
				>{h.ptys.length ? m.settings_host_action_restart_confirm() : m.settings_host_action_restart()}</Button
			>
		{/if}
	{/snippet}
</Tile>
