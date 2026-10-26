<script lang="ts">
	/** Settings ▸ Notifications & sounds ▸ Sounds: master switch, volume, "reduce", and one row per earcon with a preview. */
	import { Alert, Button, Slider, Switch } from "@codeatlasdev/ui";
	import { Play } from "@lucide/svelte";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import SectionTitle from "./SectionTitle.svelte";
	import { CATEGORIES, SPECS, type Category } from "#lib/sound/policy.ts";
	import { sound } from "#lib/sound/sound.svelte.ts";

	let p = $derived(sound.prefs);
	let off = $derived(!p.enabled);

	const set = (patch: Partial<typeof p>) => void sound.save({ ...p, ...patch });
	const setCategory = (c: Category, on: boolean) => set({ categories: { ...p.categories, [c]: on } });
</script>

<SectionTitle title="Sounds" setting="sounds">
	Short, quiet cues for the moments that matter: a stack coming up, a service failing, an agent needing you.
	Nothing plays on ordinary clicks.
</SectionTitle>

{#if !sound.systemUiSounds}
	<Alert severity="info">
		“Play user interface sound effects” is off in System Settings ▸ Sound, so Atlas stays silent. The Play
		buttons below still work.
	</Alert>
{/if}

<Group>
	<Row id="sounds" label="Play sounds" description="Master switch for every Atlas sound.">
		<Switch aria-label="Play sounds" checked={p.enabled} onchange={(v) => set({ enabled: v })} />
	</Row>
	<Row
		id="sound-volume"
		label="Sound volume"
		description="On top of your system's volume, which always wins."
	>
		<div class="vol">
			<Slider
				value={Math.round(p.volume * 100)}
				min={0}
				max={100}
				step={5}
				disabled={off}
				onchange={([v]) => set({ volume: (v ?? 0) / 100 })}
			/>
			<Button size="sm" variant="ghost" disabled={off} onclick={() => sound.preview("stack-up")}>
				{#snippet leading()}<Play size={12} />{/snippet}
				Play
			</Button>
		</div>
	</Row>
	<Row
		id="sound-reduce"
		label="Reduce sounds"
		description="Only the ones that need your attention, and quieter."
	>
		<Switch
			aria-label="Reduce sounds"
			checked={p.reduce}
			disabled={off}
			onchange={(v) => set({ reduce: v })}
		/>
	</Row>
</Group>

<Group title="Which sounds" description="Switch any of them off on its own.">
	{#each CATEGORIES as c (c)}
		<Row id="sound-cat-{c}" label={SPECS[c].label} description={SPECS[c].description}>
			<div class="ctl">
				<Button
					size="sm"
					variant="ghost"
					aria-label="Play {SPECS[c].label}"
					disabled={off}
					onclick={() => sound.preview(c)}
				>
					{#snippet leading()}<Play size={12} />{/snippet}
				</Button>
				<Switch
					aria-label={SPECS[c].label}
					checked={p.categories[c]}
					disabled={off}
					onchange={(v) => setCategory(c, v)}
				/>
			</div>
		</Row>
	{/each}
</Group>

<style>
	.vol {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 240px;
	}
	.ctl {
		display: flex;
		align-items: center;
		gap: 6px;
	}
</style>
