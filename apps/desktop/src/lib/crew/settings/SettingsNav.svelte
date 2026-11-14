<!--
  Left column of the crewmate settings dialog: the face (opens the look editor), name, engine · server, the section
  list (↑↓ move focus) and the delete button.
-->
<script lang="ts" generics="Id extends string">
	import { Pencil } from "@lucide/svelte";
	import type { Crewmate } from "#lib/crew.ts";
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import type { CrewmateState as FaceState } from "#lib/crew/crewmateFace.ts";
	import { crewServerLabel } from "#lib/crew/sshTarget.ts";

	let {
		crewmate,
		sections,
		active = $bindable(),
		closing,
		agentLabel,
		serverName,
		itemEls = $bindable([]),
		oneditlook,
		ondelete,
	}: {
		crewmate: Crewmate;
		sections: Array<{ id: Id; label: string }>;
		active: Id;
		/** The dialog is closing: animations off so the exit can finish. */
		closing: boolean;
		agentLabel: string;
		serverName: string;
		itemEls?: Array<HTMLButtonElement | null>;
		oneditlook: () => void;
		ondelete: () => void;
	} = $props();

	const faceState = $derived<FaceState>(
		crewmate.state === "needs_you" ? "needs_you" : crewmate.state === "working" ? "working" : "idle",
	);

	function keydown(e: KeyboardEvent, idx: number) {
		if (e.key === "ArrowDown") {
			e.preventDefault();
			itemEls[Math.min(idx + 1, sections.length - 1)]?.focus();
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			itemEls[Math.max(idx - 1, 0)]?.focus();
		}
	}
</script>

<!-- `closing`: the kit Dialog leaves the page only when every animation inside it has ended, and the face here
     breathes and blinks forever, so a closed dialog stayed with an invisible backdrop that swallowed every click
     (the same bug CrewmateEditor had). Closing turns the animations off so the exit can finish. -->
<nav class="settings-nav" class:closing aria-label="Settings sections">
	<div class="nav-top">
		<!-- The face is the "Customize look" button (a pencil shows on hover); no separate row for it. -->
		<button class="nav-avatar" aria-label="Customize look" title="Customize look" onclick={oneditlook}>
			<CrewmateFace seed={crewmate.id} look={crewmate.look} state={faceState} size={64} track />
			<span class="nav-avatar-edit" aria-hidden="true"><Pencil size={11} /></span>
		</button>
		<span class="nav-crewmate-name">{crewmate.name}</span>
		{#if agentLabel}
			<span class="nav-crewmate-sub" title={serverName}
				>{agentLabel}{serverName ? ` · ${crewServerLabel(serverName)}` : ""}</span
			>
		{/if}
	</div>

	<ul class="nav-list" role="list">
		{#each sections as s, i (s.id)}
			<li>
				<button
					class="nav-item"
					class:active={active === s.id}
					aria-current={active === s.id ? "page" : undefined}
					bind:this={itemEls[i]}
					onclick={() => (active = s.id)}
					onkeydown={(e) => keydown(e, i)}>{s.label}</button
				>
			</li>
		{/each}
	</ul>

	<div class="nav-bottom">
		<button class="nav-delete" onclick={ondelete}>Delete crewmate</button>
	</div>
</nav>

<style>
	.closing,
	.closing :global(*) {
		animation: none !important;
		transition: none !important;
	}
	.settings-nav {
		display: flex;
		flex-direction: column;
		border-right: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		overflow-y: auto;
	}

	.nav-top {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 5px;
		padding: 28px 12px 16px;
	}

	.nav-avatar {
		position: relative;
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
		border-radius: 50%;
		margin-bottom: 4px;
	}
	/* Always shown: hidden until hover, nothing said the face could be customized. */
	.nav-avatar-edit {
		position: absolute;
		right: -2px;
		bottom: -2px;
		display: inline-grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: var(--ca-surface-raised, var(--ca-surface-base));
		border: 1px solid var(--ca-outline-subtle);
		color: var(--ca-text-primary);
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.35);
		transition:
			background 120ms,
			color 120ms;
	}
	.nav-avatar::after {
		content: "";
		position: absolute;
		inset: -4px;
		border-radius: 50%;
		border: 2px solid transparent;
		transition: border-color 120ms;
		pointer-events: none;
	}
	.nav-avatar:hover::after {
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, transparent);
	}
	.nav-avatar:hover .nav-avatar-edit,
	.nav-avatar:focus-visible .nav-avatar-edit {
		background: var(--ca-brand-main);
		color: var(--ca-text-on-brand);
	}
	.nav-avatar:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: 2px;
	}

	.nav-crewmate-name {
		font-size: 13px;
		font-weight: 600;
		text-align: center;
		color: var(--ca-text-primary);
	}

	.nav-crewmate-sub {
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-align: center;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.nav-list {
		list-style: none;
		margin: 0;
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 1px;
		flex: 1;
	}

	.nav-item {
		display: block;
		width: 100%;
		text-align: left;
		font: inherit;
		font-size: 13px;
		padding: 7px 10px;
		border: none;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
		transition:
			background 80ms,
			color 80ms;
	}
	.nav-item:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.nav-item.active,
	.nav-item[aria-current="page"] {
		background: color-mix(in oklab, var(--ca-brand-main) 12%, var(--ca-surface-base));
		color: var(--ca-text-primary);
		font-weight: 500;
	}
	.nav-item:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: -2px;
	}

	.nav-bottom {
		padding: 8px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		margin-top: auto;
	}

	.nav-delete {
		display: block;
		width: 100%;
		text-align: left;
		font: inherit;
		font-size: 12px;
		padding: 6px 10px;
		border: none;
		border-radius: 7px;
		background: none;
		color: var(--ca-semantic-error);
		cursor: default;
	}
	.nav-delete:hover {
		background: color-mix(in oklab, var(--ca-semantic-error) 10%, transparent);
	}
</style>
