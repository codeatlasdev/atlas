<script lang="ts">
	/** Settings ▸ Notifications & sounds ▸ Sounds: master switch, volume, "reduce", and one row per earcon with a preview. */
	import { Alert, Button, Slider, Switch } from "@codeatlasdev/ui";
	import { Play } from "@lucide/svelte";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import SectionTitle from "./SectionTitle.svelte";
	import { CATEGORIES, SPECS, type Category } from "#lib/sound/policy.ts";
	import { sound } from "#lib/sound/sound.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let p = $derived(sound.prefs);
	let off = $derived(!p.enabled);

	const set = (patch: Partial<typeof p>) => void sound.save({ ...p, ...patch });
	const setCategory = (c: Category, on: boolean) => set({ categories: { ...p.categories, [c]: on } });
</script>

<SectionTitle title={m.settings_general_sound_sounds()} setting="sounds">
	{m.settings_general_sound_short_quiet_cues_for_the()}
</SectionTitle>

{#if !sound.systemUiSounds}
	<Alert severity="info">
		{m.settings_general_sound_play_user_interface_sound_effects()}
	</Alert>
{/if}

<Group>
	<Row id="sounds" label={m.settings_general_sound_play_sounds()} description={m.settings_general_sound_master_switch_for_every_atlas()}>
		<Switch aria-label={m.settings_general_sound_play_sounds()} checked={p.enabled} onchange={(v) => set({ enabled: v })} />
	</Row>
	<Row
		id="sound-volume"
		label={m.settings_general_sound_sound_volume()}
		description={m.settings_general_sound_on_top_of_your_system()}
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
				{m.settings_general_sound_play()}
			</Button>
		</div>
	</Row>
	<Row
		id="sound-reduce"
		label={m.settings_general_sound_reduce_sounds()}
		description={m.settings_general_sound_only_the_ones_that_need()}
	>
		<Switch
			aria-label={m.settings_general_sound_reduce_sounds()}
			checked={p.reduce}
			disabled={off}
			onchange={(v) => set({ reduce: v })}
		/>
	</Row>
</Group>

<Group title={m.settings_general_sound_which_sounds()} description={m.settings_general_sound_switch_any_of_them_off()}>
	{#each CATEGORIES as c (c)}
		<Row id="sound-cat-{c}" label={SPECS[c].label} description={SPECS[c].description}>
			<div class="ctl">
				<Button
					size="sm"
					variant="ghost"
					aria-label={m.settings_general_sound_play_label({ sound: SPECS[c].label })}
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
