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
	title="Agent dock"
	description="A small pill that floats above your other apps. Press the hotkey from anywhere to message an agent; it works with the Atlas window closed."
>
	{#if prefs}
		<Row
			id="dock-enabled"
			label="Show the dock"
			description="Keep the pill on screen while Atlas runs. Hide it any time with the hide key, the pill's hide button or right-click menu, or the tray menu; the choice is remembered."
		>
			<Switch
				aria-label="Show the dock"
				checked={prefs.enabled}
				onchange={(enabled) => prefs && save({ ...prefs, enabled })}
			/>
		</Row>
		<Row
			id="dock-hotkey"
			label="Message an agent"
			description="Opens the dock's composer from any app. The text goes to the agent you last used, or the one you @mention."
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
					>Clear</Button
				>
			{/if}
		</Row>
		<Row
			id="dock-toggle-hotkey"
			label="Hide or show the dock"
			description="Works even while the dock is hidden, so it is always the way back. The tray menu has Show Agent Dock too."
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
					>Clear</Button
				>
			{/if}
		</Row>
	{/if}
</Group>
