<script lang="ts">
	import { Button, Kbd, toast } from "@codeatlasdev/ui";
	import { Undo2, X } from "@lucide/svelte";
	import SearchField from "./SearchField.svelte";
	import { errText } from "#lib/api.ts";
	import { DEFAULT_KEYMAP, IS_MAC, kbd, label, strokeOf } from "#lib/keys.ts";
	import { highlight, fuzzy } from "#lib/fuzzy.ts";
	import {
		bindKey,
		bindingLabels,
		keymap,
		registeredCommands,
		resetKeymap,
		unbindKey,
	} from "#lib/keymap.svelte.ts";

	let query = $state("");
	/** Command id waiting for its new keystroke. */
	let recording = $state<string | null>(null);

	const customized = $derived(Object.keys(keymap.user).length > 0);

	let rows = $derived.by(() => {
		const q = query.trim();
		return registeredCommands()
			.filter((c) => !/^(atlas::RecentWorktree|pane::ActivateItem\d)/.test(c.id) || !q)
			.map((c) => ({ c, s: q ? fuzzy(q, c.label, c.id) : 1 }))
			.filter((x) => x.s > 0)
			.sort((a, b) => (q ? b.s - a.s : a.c.label.localeCompare(b.c.label)))
			.map((x) => x.c);
	});

	const isDefault = (id: string) => Object.values(DEFAULT_KEYMAP).includes(id);

	function record(e: KeyboardEvent) {
		const id = recording;
		if (!id) return;
		e.preventDefault();
		e.stopPropagation();
		if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey) {
			recording = null;
			return;
		}
		const stroke = strokeOf(e);
		// Modifier alone, or a key we cannot name: keep waiting.
		if (!stroke) return;
		// A bare or shift-only key would steal typing everywhere; require ⌘, ⌃ or ⌥ (or an F-key).
		if (!/(^|-)(ctrl|alt|cmd)-|^f\d+$/.test(stroke)) {
			toast.info(
				`Include ${IS_MAC ? "⌘, ⌃ or ⌥" : "Ctrl or Alt"} so the shortcut does not clash with typing`,
			);
			return;
		}
		recording = null;
		bindKey(id, stroke)
			.then((r) => {
				if (r?.displaced) {
					const other = registeredCommands().find((c) => c.id === r.displaced)?.label ?? r.displaced;
					toast.info(`${label(stroke)} now runs this command. “${other}” lost it.`);
				}
			})
			.catch((err) => toast.error(errText(err)));
	}

	function clear(id: string) {
		unbindKey(id).catch((err) => toast.error(errText(err)));
	}
</script>

<svelte:window onkeydowncapture={record} />

<div class="shortcuts">
	<div class="bar">
		<SearchField placeholder="Search commands" aria-label="Search commands" bind:value={query} />
		<Button variant="ghost" size="sm" disabled={!customized} onclick={() => resetKeymap()}>
			{#snippet leading()}<Undo2 size={13} />{/snippet}
			Reset all to defaults
		</Button>
	</div>

	<div class="list">
		{#each rows as c (c.id)}
			{@const keys = bindingLabels(c.id)}
			<div class="row" class:rec={recording === c.id} data-setting={c === rows[0] ? "keymap" : undefined}>
				<div class="what">
					<span class="name"
						>{#each highlight(query.trim(), c.label) as r, k (k)}{#if r.hit}<mark>{r.text}</mark
								>{:else}{r.text}{/if}{/each}</span
					>
				</div>
				<div class="keys">
					{#if recording === c.id}
						<span class="prompt">Press the new shortcut… <Kbd>{kbd("escape")}</Kbd> to cancel</span>
					{:else}
						<button
							class="keybtn"
							title="Click, then press the new shortcut"
							onclick={() => (recording = c.id)}
						>
							{#if keys.length}{#each keys as k (k)}<Kbd>{k}</Kbd>{/each}{:else}<span class="none"
									>{isDefault(c.id) ? "No shortcut" : "Add shortcut"}</span
								>{/if}
						</button>
						{#if keys.length}
							<button class="x" aria-label="Remove this shortcut" onclick={() => clear(c.id)}
								><X size={12} /></button
							>
						{/if}
					{/if}
				</div>
			</div>
		{:else}
			<p class="empty">No command matches “{query.trim()}”.</p>
		{/each}
	</div>
	<p class="hint">If you pick a shortcut that another command already uses, that command loses it.</p>
</div>

<style>
	.shortcuts {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-height: 0;
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.list {
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 38px;
		padding: 6px 12px;
		transition: background 120ms ease-out;
	}
	.row + .row {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.row:hover,
	.row.rec {
		background: var(--ca-action-hover);
	}
	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		line-height: 1.3;
	}
	.name {
		font-size: 13px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	mark {
		background: none;
		color: var(--ca-brand-main);
		font-weight: 650;
	}
	.keys {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}
	.keybtn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		min-width: 56px;
		justify-content: flex-end;
		height: 26px;
		padding: 0 6px;
		border: 0.5px solid transparent;
		border-radius: 7px;
		background: none;
		font: inherit;
	}
	.keybtn:hover {
		border-color: var(--ca-outline-subtle);
		background: var(--ca-surface-base);
	}
	.none {
		font-size: 11.5px;
		/* "Add shortcut" is an action, not a disabled control: body-text contrast */
		color: var(--ca-text-secondary);
	}
	.prompt {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-brand-main);
		animation: blink 1.1s ease-in-out infinite;
	}
	@keyframes blink {
		50% {
			opacity: 0.55;
		}
	}
	.x {
		display: inline-grid;
		place-items: center;
		width: 20px;
		height: 20px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-disabled);
		opacity: 0;
		transition: opacity 120ms ease-out;
	}
	.row:hover .x {
		opacity: 1;
	}
	.x:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.empty {
		margin: 0;
		padding: 20px;
		text-align: center;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.hint {
		margin: 0;
		font-size: 11.5px;
		color: var(--ca-text-hint);
	}
</style>
