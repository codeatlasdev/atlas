<script lang="ts">
	/**
	 * ModeBadge — a crewmate's permission mode as a small glyph (lock, pencil, shield) with or without its name. Only
	 * Bypass is coloured (amber); the other modes stay neutral so the colour means "this one runs unattended".
	 * Used in the chat header and the crewmate list; the composer's picker has its own control.
	 */
	import { Lock, Pencil, Shield } from "@lucide/svelte";
	import type { PermissionMode } from "#lib/crew.ts";
	import { modeIsAmber, modeLabel } from "#lib/crew/permissionMode.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		mode: PermissionMode;
		/** Show the mode's name next to the glyph; without it the glyph carries the name as its accessible label. */
		showLabel?: boolean;
		size?: number;
	}

	let { mode, showLabel = true, size = 12 }: Props = $props();

	const name = $derived(modeLabel(mode));
	const full = $derived(m.crew_permission_badge_label({ mode: name }));
</script>

<span
	class="mode-badge"
	class:amber={modeIsAmber(mode)}
	data-mode={mode}
	title={full}
	role={showLabel ? undefined : "img"}
	aria-label={showLabel ? undefined : full}
>
	{#if mode === "ask"}<Lock {size} aria-hidden="true" />
	{:else if mode === "edits"}<Pencil {size} aria-hidden="true" />
	{:else}<Shield {size} aria-hidden="true" />{/if}
	{#if showLabel}<span>{name}</span>{/if}
</span>

<style>
	.mode-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex: none;
		font-size: 11.5px;
		line-height: 1;
		color: var(--ca-text-secondary);
	}
	.mode-badge.amber {
		color: var(--ca-semantic-warning);
	}
</style>
