<script lang="ts">
	/**
	 * CrewPalette — ⌘K command palette for the Crew workspace.
	 * Opened by the Search button or ⌘K while the workspace is focused.
	 * Closed by Esc or click-outside (Dialog handles both).
	 */
	import { Dialog, Kbd } from "@codeatlasdev/ui";
	import { Search, Plus, Plug, Settings, Keyboard } from "@lucide/svelte";
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import { filterRows, cmdKeyIndex, keycapLabel } from "#lib/crew/crewPalette.ts";
	import type { PaletteRow } from "#lib/crew/crewPalette.ts";

	interface Props {
		open: boolean;
		crewmates: Array<{
			id: string;
			hostKey: string;
			name: string;
			role: string;
			look?: unknown;
			state: string;
		}>;
		hasShortcuts?: boolean;
		onclose: () => void;
		onSelectCrewmate: (hostKey: string, crewmateId: string) => void;
		onNewCrewmate: () => void;
		onIntegrations: () => void;
		onServerSettings: () => void;
		onHelp?: () => void;
	}

	let {
		open = $bindable(),
		crewmates,
		hasShortcuts = false,
		onclose,
		onSelectCrewmate,
		onNewCrewmate,
		onIntegrations,
		onServerSettings,
		onHelp,
	}: Props = $props();

	let query = $state("");
	let selectedIndex = $state(0);
	let inputEl = $state<HTMLInputElement | null>(null);

	// Build the full ordered row list: crewmates first, then commands.
	const crewmateRows = $derived(
		crewmates.map((b): PaletteRow => ({
			kind: "crewmate",
			id: b.id,
			hostKey: b.hostKey,
			name: b.name,
			subtitle: b.role ? b.role.slice(0, 80) : "",
		})),
	);

	const commandRows = $derived((): PaletteRow[] => {
		const rows: PaletteRow[] = [
			{ kind: "command", id: "new", name: "New crewmate" },
			{ kind: "command", id: "integrations", name: "Integrations" },
			{ kind: "command", id: "settings", name: "Crew server settings" },
		];
		if (onHelp) rows.push({ kind: "command", id: "help", name: "Keyboard shortcuts", subtitle: "Help" });
		return rows;
	});

	const allRows = $derived([...crewmateRows, ...commandRows()]);
	const visible = $derived(filterRows(allRows, query));

	// Clamp selection when list shrinks.
	$effect(() => {
		if (selectedIndex >= visible.length) selectedIndex = Math.max(0, visible.length - 1);
	});

	// Reset query + selection on open/close; focus input when opening.
	$effect(() => {
		if (!open) {
			query = "";
			selectedIndex = 0;
		} else {
			const id = requestAnimationFrame(() => inputEl?.focus());
			return () => cancelAnimationFrame(id);
		}
	});

	function activate(row: PaletteRow) {
		open = false;
		if (row.kind === "crewmate") {
			onSelectCrewmate(row.hostKey, row.id);
		} else {
			switch (row.id) {
				case "new":
					onNewCrewmate();
					break;
				case "integrations":
					onIntegrations();
					break;
				case "settings":
					onServerSettings();
					break;
				case "help":
					onHelp?.();
					break;
			}
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "ArrowDown") {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, visible.length - 1);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === "Enter") {
			e.preventDefault();
			const row = visible[selectedIndex];
			if (row) activate(row);
		} else if (e.metaKey || e.ctrlKey) {
			const idx = cmdKeyIndex(e.key);
			if (idx >= 0 && idx < visible.length) {
				e.preventDefault();
				activate(visible[idx]);
			}
		}
	}

	function getCrewmateEntry(row: CrewmateRow) {
		return crewmates.find((b) => b.id === row.id && b.hostKey === row.hostKey);
	}

	type CrewmateRow = Extract<PaletteRow, { kind: "crewmate" }>;
	type CommandRow = Extract<PaletteRow, { kind: "command" }>;

	const commandIcons: Record<string, typeof Plus> = {
		new: Plus,
		integrations: Plug,
		settings: Settings,
		help: Keyboard,
	};
</script>

<Dialog bind:open title="Crew search" chromeless size="md">
	<!-- Search bar -->
	<div class="pal-search">
		<Search size={16} class="pal-search__icon" aria-hidden="true" />
		<input
			bind:this={inputEl}
			bind:value={query}
			type="search"
			placeholder="Search"
			class="pal-search__input"
			aria-label="Search crewmates and commands"
			autocomplete="off"
			spellcheck={false}
			onkeydown={handleKeydown}
		/>
	</div>
	<div class="pal-divider" role="separator" aria-hidden="true"></div>

	<!-- Row list -->
	<!-- `closing`: see CrewmateEditor.svelte; endless face animations kept the closed Dialog (and its backdrop) on screen. -->
	<div class="pal-list" class:closing={!open} role="listbox" aria-label="Crew and commands">
		{#each visible as row, i (row.kind + ":" + row.id)}
			{@const active = i === selectedIndex}
			{@const cap = keycapLabel(i)}
			<button
				class="pal-row"
				class:pal-row--active={active}
				role="option"
				aria-selected={active}
				onclick={() => activate(row)}
				onmousemove={() => (selectedIndex = i)}
			>
				<!-- Leading slot: 40px -->
				<div class="pal-lead" aria-hidden="true">
					{#if row.kind === "crewmate"}
						{@const entry = getCrewmateEntry(row)}
						{#if entry}
							<CrewmateFace
								seed={entry.id}
								look={entry.look}
								state={entry.state === "needs_you" ? "needs_you" : "idle"}
								size={36}
							/>
						{/if}
					{:else}
						{@const Icon = commandIcons[row.id] ?? Settings}
						<div class="pal-cmd-tile">
							<Icon size={16} />
						</div>
					{/if}
				</div>

				<!-- Text -->
				<div class="pal-text">
					<span class="pal-title">{row.name}</span>
					{#if row.kind === "crewmate" && row.subtitle}
						<span class="pal-sub">{row.subtitle}</span>
					{:else if row.kind === "command" && row.subtitle}
						<span class="pal-sub">{row.subtitle}</span>
					{/if}
				</div>

				<!-- Keycap -->
				{#if cap}
					<Kbd class="pal-kbd">⌘{cap}</Kbd>
				{/if}
			</button>
		{/each}

		{#if visible.length === 0}
			<p class="pal-empty">No results for "{query}"</p>
		{/if}
	</div>
</Dialog>

<style>
	.pal-list.closing :global(*) {
		animation: none !important;
		transition: none !important;
	}
	/* The Dialog chromeless body has no padding; we own the layout. */
	:global(.ca-dialog .ca-dialog__body) {
		padding: 0;
	}

	.pal-search {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
	}

	:global(.pal-search__icon) {
		color: var(--ca-text-hint);
		flex-shrink: 0;
	}

	.pal-search__input {
		flex: 1;
		background: none;
		border: none;
		outline: none;
		font-size: 15px;
		color: var(--ca-text-primary);
		font-family: inherit;
		/* Remove default search input chrome */
		appearance: none;
	}

	.pal-search__input::placeholder {
		color: var(--ca-text-hint);
	}

	/* suppress webkit search cancel button */
	.pal-search__input::-webkit-search-cancel-button {
		display: none;
	}

	.pal-divider {
		height: 1px;
		background: var(--ca-outline-subtle);
		flex-shrink: 0;
	}

	.pal-list {
		padding: 6px;
		overflow-y: auto;
		max-height: 420px;
	}

	.pal-row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 56px;
		padding: 8px 10px;
		border: none;
		background: none;
		border-radius: 12px;
		cursor: default;
		text-align: left;
		color: var(--ca-text-primary);
		transition: background 100ms;
	}

	.pal-row--active,
	.pal-row:hover {
		background: var(--ca-surface-raised, var(--ca-vibrancy-fill));
	}

	.pal-lead {
		width: 40px;
		height: 40px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.pal-cmd-tile {
		width: 36px;
		height: 36px;
		border-radius: 10px;
		background: var(--ca-surface-raised, var(--ca-vibrancy-fill));
		border: 1px solid var(--ca-outline-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--ca-text-secondary);
		flex-shrink: 0;
	}

	.pal-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.pal-title {
		font-size: 15px;
		font-weight: 500;
		line-height: 1.3;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.pal-sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	:global(.pal-kbd) {
		flex-shrink: 0;
		--ca-kbd-size: 22px;
	}

	.pal-empty {
		text-align: center;
		font-size: 13px;
		color: var(--ca-text-hint);
		padding: 32px 16px;
		margin: 0;
	}
</style>
