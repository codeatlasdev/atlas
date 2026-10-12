<script lang="ts">
	import { Button, Dialog, SegmentedControl, toast } from "@codeatlasdev/ui";
	import { Check, ImageIcon, Search } from "@lucide/svelte";
	import { api, errText } from "#lib/api.ts";
	import { lucideTable, LucideIcon, type IconNode } from "#lib/lucide.ts";
	import { SWATCH_HUES, searchIcons, type IconChoice } from "#lib/shell/identity.ts";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";

	/** Quick picks for the emoji tab; any emoji can be typed or pasted (⌃⌘Space opens the macOS picker). */
	const EMOJIS =
		"🚀 ⚡ 🔥 🧪 🛠️ 📦 🌐 🧠 🤖 🎯 💡 🔒 📊 🎨 🎮 🛒 🏠 🏢 💬 📱 🖥️ ☁️ 🐳 🦀 🐍 🍃 🌱 🌊 ⭐ 💎 🧩 📚 📈 💰 🔔 📷 🎵 ✈️ 🚗 🏥 🎓 🍔 ⚽ 🐙 🦊 🐻 🦄".split(
			" ",
		);

	const id = $derived(identities.editing);
	const project = $derived(store.projects.find((p) => p.id === id) ?? null);
	const logo = $derived(id ? identities.logoOf(id) : null);

	let tab = $state("emoji");
	let icon = $state<IconChoice | undefined>();
	let hue = $state<number | undefined>();
	let emoji = $state("");
	let query = $state("");
	let names = $state<string[]>([]);
	let nodes = $state<Record<string, IconNode>>({});
	let saving = $state(false);

	// Re-seed from the stored choice every time the dialog opens for a project.
	$effect(() => {
		if (!id) return;
		const cur = store.projects.find((p) => p.id === id)?.identity;
		icon = cur?.icon;
		hue = cur?.hue;
		emoji = cur?.icon?.kind === "emoji" ? cur.icon.value : "";
		query = "";
		tab = cur?.icon?.kind === "lucide" ? "icons" : "emoji";
	});

	$effect(() => {
		if (!id || names.length) return;
		void lucideTable().then((t) => {
			nodes = t;
			names = Object.keys(t);
		});
	});

	const shown = $derived(searchIcons(names, query));
	const autoHue = $derived(id ? (shell.hues[id] ?? 0) : 0);
	/** What the tile will look like: preview the draft without touching the stored identity. */
	const dirty = $derived.by(() => {
		const cur = project?.identity;
		return (
			JSON.stringify([icon ?? null, hue ?? null]) !== JSON.stringify([cur?.icon ?? null, cur?.hue ?? null])
		);
	});

	function pickEmoji(v: string) {
		emoji = v;
		icon = v.trim() ? { kind: "emoji", value: v.trim() } : undefined;
	}

	async function save(next: { icon?: IconChoice; hue?: number } | null = { icon, hue }) {
		if (!id) return;
		saving = true;
		try {
			await api.setIdentity(id, next && (next.icon || next.hue !== undefined) ? next : null);
			await store.refresh();
			identities.editing = null;
		} catch (e) {
			toast.error(errText(e));
		} finally {
			saving = false;
		}
	}
</script>

<Dialog
	open={!!id}
	onOpenChange={(o) => !o && (identities.editing = null)}
	title="Change icon and color"
	description={project?.name}
	size="md"
>
	{#if id}
		<div class="preview" style:--c={`oklch(0.7 0.15 ${hue ?? autoHue})`}>
			<span class="draft" style:--c={`oklch(0.7 0.15 ${hue ?? autoHue})`}>
				{#if icon?.kind === "emoji"}<span class="e">{icon.value}</span>
				{:else if icon?.kind === "lucide" && nodes[icon.value]}<LucideIcon
						iconNode={nodes[icon.value]}
						size={26}
					/>
				{:else if icon?.kind === "image" && logo}<img src={logo.url} alt="" />
				{:else}<ProjectIcon {id} size={44} />{/if}
			</span>
			<div class="swatches" role="radiogroup" aria-label="Color">
				<button
					class="sw auto"
					class:on={hue === undefined}
					role="radio"
					aria-checked={hue === undefined}
					aria-label="Automatic color"
					title="Automatic"
					style:background={`oklch(0.68 0.16 ${autoHue})`}
					onclick={() => (hue = undefined)}
				>
					{#if hue === undefined}<Check size={12} />{/if}
				</button>
				{#each SWATCH_HUES as h (h)}
					<button
						class="sw"
						class:on={hue === h}
						role="radio"
						aria-checked={hue === h}
						aria-label="Hue {h}"
						style:background={`oklch(0.68 0.16 ${h})`}
						onclick={() => (hue = h)}
					>
						{#if hue === h}<Check size={12} />{/if}
					</button>
				{/each}
			</div>
		</div>

		<SegmentedControl
			bind:value={tab}
			size="sm"
			fullWidth
			aria-label="Icon source"
			items={[
				{ value: "emoji", label: "Emoji" },
				{ value: "icons", label: "Icons" },
				{ value: "logo", label: "Project logo" },
			]}
		/>

		<div class="body">
			{#if tab === "emoji"}
				<input
					class="field"
					placeholder="Type or paste an emoji (⌃⌘Space)"
					value={emoji}
					maxlength="16"
					oninput={(e) => pickEmoji(e.currentTarget.value)}
				/>
				<div class="grid emojis">
					{#each EMOJIS as e (e)}
						<button
							class="cell"
							class:on={icon?.kind === "emoji" && icon.value === e}
							onclick={() => pickEmoji(e)}>{e}</button
						>
					{/each}
				</div>
			{:else if tab === "icons"}
				<div class="search">
					<Search size={13} />
					<input class="bare" placeholder="Search {names.length || ''} icons" bind:value={query} />
				</div>
				<div class="grid lucides">
					{#each shown as n (n)}
						<button
							class="cell"
							class:on={icon?.kind === "lucide" && icon.value === n}
							title={n}
							aria-label={n}
							onclick={() => (icon = { kind: "lucide", value: n })}
						>
							{#if nodes[n]}<LucideIcon iconNode={nodes[n]} size={18} />{/if}
						</button>
					{/each}
					{#if names.length && !shown.length}<p class="none">No icon matches “{query}”.</p>{/if}
				</div>
			{:else if logo}
				<button
					class="logo"
					class:on={icon?.kind === "image" && icon.value === logo.path}
					onclick={() => (icon = { kind: "image", value: logo.path })}
				>
					<img src={logo.url} alt="" />
					<span><b>Use project logo</b><small class="mono">{logo.path}</small></span>
				</button>
			{:else}
				<p class="none">
					<ImageIcon size={14} /> No logo found in this repo (favicon, app icon, assets/logo…).
				</p>
			{/if}
		</div>
	{/if}
	{#snippet actions()}
		<Button variant="ghost" disabled={saving || (!project?.identity && !dirty)} onclick={() => save(null)}>
			Reset
		</Button>
		<Button variant="ghost" onclick={() => (identities.editing = null)}>Cancel</Button>
		<Button disabled={saving || !dirty} onclick={() => save()}>Save</Button>
	{/snippet}
</Dialog>

<style>
	.preview {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-bottom: 12px;
	}
	.draft {
		display: inline-grid;
		place-items: center;
		flex: none;
		width: 44px;
		height: 44px;
		border-radius: 13px;
		color: var(--c);
		background: color-mix(in oklab, var(--c) 18%, transparent);
	}
	.draft .e {
		font-size: 27px;
		line-height: 1;
	}
	.draft img {
		width: 34px;
		height: 34px;
		object-fit: contain;
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.sw {
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		color: white;
		cursor: pointer;
		box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 0.2);
	}
	.sw.auto {
		outline: 1px dashed var(--ca-outline-subtle);
		outline-offset: 2px;
	}
	.sw.on {
		box-shadow:
			0 0 0 2px var(--ca-surface-raised),
			0 0 0 3.5px var(--ca-text-primary);
	}
	.body {
		margin-top: 10px;
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-height: 220px;
	}
	.field,
	.search {
		height: 30px;
		padding: 0 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 13px;
		user-select: text;
		-webkit-user-select: text;
	}
	.search {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--ca-text-secondary);
	}
	.bare {
		flex: 1;
		min-width: 0;
		border: 0;
		outline: none;
		background: none;
		font: inherit;
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
	}
	.grid {
		display: grid;
		gap: 4px;
		max-height: 210px;
		overflow-y: auto;
	}
	.grid.emojis {
		grid-template-columns: repeat(8, 1fr);
	}
	.grid.lucides {
		grid-template-columns: repeat(8, 1fr);
	}
	.cell {
		display: grid;
		place-items: center;
		height: 34px;
		border: 0;
		border-radius: 8px;
		background: none;
		font-size: 19px;
		color: var(--ca-text-primary);
		cursor: pointer;
	}
	.cell:hover {
		background: var(--ca-vibrancy-fill);
	}
	.cell.on {
		background: color-mix(in oklab, var(--ca-brand-main) 20%, transparent);
		box-shadow: inset 0 0 0 1px var(--ca-brand-main);
	}
	.logo {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		background: none;
		font: inherit;
		text-align: left;
		color: var(--ca-text-primary);
		cursor: pointer;
	}
	.logo.on {
		border-color: var(--ca-brand-main);
		background: color-mix(in oklab, var(--ca-brand-main) 10%, transparent);
	}
	.logo img {
		width: 36px;
		height: 36px;
		object-fit: contain;
	}
	.logo span {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.logo small {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.none {
		grid-column: 1 / -1;
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 8px 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
