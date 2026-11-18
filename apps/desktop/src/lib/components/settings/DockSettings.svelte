<script lang="ts">
	/** Settings ▸ Agents ▸ Agent dock: the floating pill, and the hotkeys that talk to it. */
	import { Button, Switch, toast } from "@codeatlasdev/ui";
	import { invoke } from "@tauri-apps/api/core";
	import { onMount } from "svelte";
	import { errText } from "#lib/api.ts";
	import type { DockPrefs } from "#lib/dock.ts";
	import { dockKeyFrom } from "#lib/dockKey.ts";
	import { IS_MAC, kbd } from "#lib/keys.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { m } from "#lib/i18n/m.ts";

	type Field = "summon" | "toggle";

	let prefs = $state<DockPrefs | null>(null);
	let recording = $state<Field | null>(null);

	onMount(() => {
		void invoke<DockPrefs>("dock_prefs").then((p) => (prefs = p));
	});

	/** Apply and save through Rust: it checks the key, registers it with the system, and only then writes it. */
	async function save(next: DockPrefs) {
		try {
			prefs = await invoke<DockPrefs>("dock_prefs_set", { prefs: next });
		} catch (e) {
			toast.error(errText(e));
		}
	}

	function record(e: KeyboardEvent) {
		if (!recording || !prefs) return;
		e.preventDefault();
		e.stopPropagation();
		const r = dockKeyFrom(e, IS_MAC);
		if (r.kind === "wait") return;
		if (r.kind === "refuse") return void toast.info(r.reason);
		const field = recording;
		recording = null;
		if (r.kind === "stroke") void save({ ...prefs, [field]: r.stroke });
	}
</script>

<svelte:window onkeydowncapture={record} />

<Group
	title={m.settings_general_dock_agent_dock()}
	description={m.settings_general_dock_a_small_pill_that_floats()}
>
	{#if prefs}
		<Row
			id="dock-enabled"
			label={m.settings_general_dock_show_the_dock()}
			description={m.settings_general_dock_keep_the_pill_on_screen()}
		>
			<Switch
				aria-label={m.settings_general_dock_show_the_dock()}
				checked={prefs.enabled}
				onchange={(enabled) => prefs && save({ ...prefs, enabled })}
			/>
		</Row>
		<Row
			id="dock-hotkey"
			label={m.settings_general_dock_message_an_agent()}
			description={m.settings_general_dock_opens_the_dock_s_composer()}
		>
			<Button
				size="sm"
				variant={recording === "summon" ? "filled" : "outlined"}
				onclick={() => (recording = recording === "summon" ? null : "summon")}
			>
				{recording === "summon" ? "Press a key…" : prefs.summon ? kbd(prefs.summon) : "Not set"}
			</Button>
			{#if prefs.summon && recording !== "summon"}
				<Button size="sm" variant="ghost" onclick={() => prefs && save({ ...prefs, summon: "" })}
					>{m.settings_general_dock_clear()}</Button
				>
			{/if}
		</Row>
		<Row
			id="dock-toggle-hotkey"
			label={m.settings_general_dock_hide_or_show_the_dock()}
			description={m.settings_general_dock_works_even_while_the_dock()}
		>
			<Button
				size="sm"
				variant={recording === "toggle" ? "filled" : "outlined"}
				onclick={() => (recording = recording === "toggle" ? null : "toggle")}
			>
				{recording === "toggle" ? "Press a key…" : prefs.toggle ? kbd(prefs.toggle) : "Not set"}
			</Button>
			{#if prefs.toggle && recording !== "toggle"}
				<Button size="sm" variant="ghost" onclick={() => prefs && save({ ...prefs, toggle: "" })}
					>{m.settings_general_dock_clear()}</Button
				>
			{/if}
		</Row>
	{/if}
</Group>
