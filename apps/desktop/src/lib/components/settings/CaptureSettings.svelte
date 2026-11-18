<script lang="ts">
	/**
	 * Settings ▸ Capture: screenshots and screen recordings from a hotkey. Rust owns every rule (`shots_prefs_set`
	 * validates and registers the keys); this page only shows what Rust says and never keeps a value Rust refused:
	 * `prefs` is assigned from Rust's reply alone. Recording a hotkey: click its button, press the chord; Esc cancels.
	 */
	import { Button, Select, Switch, toast } from "@codeatlasdev/ui";
	import { FileImage, Film, Image } from "@lucide/svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { onMount } from "svelte";
	import { errText } from "#lib/api.ts";
	import type { DockPrefs } from "#lib/dock.ts";
	import { tilde } from "#lib/format.ts";
	import { IS_MAC, kbd, revealLabel } from "#lib/keys.ts";
	import Group from "#lib/settings/Group.svelte";
	import NumInput from "#lib/settings/NumInput.svelte";
	import Row from "#lib/settings/Row.svelte";
	import * as history from "#lib/shots/history.ts";
	import {
		KEY_FIELDS,
		activePresetId,
		applyPreset,
		clashes,
		cornerOptions,
		historyGroups,
		historyLabel,
		keyLabel,
		presetAvailable,
		presetLabel,
		presetNote,
		qualityOptions,
		recordingResult,
		scaleOptions,
		systemKeyArgs,
		systemKeysClash,
		systemKeysUsed,
		timerOptions,
		verdictText,
	} from "#lib/shots/settings.ts";
	import type { CapturePrefs, Entry, Keys, Preset, RecordCapabilities, Verdict } from "#lib/shots/types.ts";
	import { dateTime, list } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";

	const KEY = Object.fromEntries(KEY_FIELDS.map((k) => [k.field, k])) as Record<
		keyof Keys,
		(typeof KEY_FIELDS)[number]
	>;
	const CUSTOM = "custom";
	const DEFAULT_DIR = "~/Pictures/Atlas Captures";
	const ICON = { screenshot: Image, recording: Film, gif: FileImage } as const;

	let prefs = $state<CapturePrefs | null>(null);
	let presets = $state<Preset[]>([]);
	let dockKeys = $state<string[]>([]);
	let granted = $state<boolean | null>(null);
	let caps = $state<RecordCapabilities | null>(null);
	let recording = $state<keyof Keys | null>(null);
	let verdicts = $state<Partial<Record<keyof Keys, Verdict>>>({});
	let entries = $state<Entry[]>([]);
	let historyError = $state<string | null>(null);
	/** The preset chosen in the list but not applied yet: its note is on screen BEFORE the keys change. */
	let picked = $state<string | null>(null);

	const activeId = $derived(prefs ? (activePresetId(prefs.keys, presets) ?? CUSTOM) : CUSTOM);
	const shownId = $derived(picked ?? activeId);
	const shownPreset = $derived(presets.find((p) => p.id === shownId) ?? null);
	const pickedAvailable = $derived(!shownPreset || !prefs || presetAvailable(shownPreset, prefs));
	const usedSystem = $derived(prefs ? systemKeysUsed(prefs.keys) : []);
	const clash = $derived(prefs ? clashes(prefs.keys, dockKeys) : null);
	const groups = $derived(historyGroups(entries, Date.now()));

	onMount(() => {
		void load();
	});

	async function load() {
		void invoke<{ granted: boolean }>("shots_permission").then(
			(p) => (granted = p.granted),
			(e) => toast.error(errText(e)),
		);
		void invoke<DockPrefs>("dock_prefs").then(
			(d) => (dockKeys = [d.summon, d.toggle]),
			() => {},
		);
		void invoke<RecordCapabilities>("shots_record_capabilities").then(
			(c) => (caps = c),
			(e) => toast.error(errText(e)),
		);
		void refreshHistory();
		try {
			[prefs, presets] = await Promise.all([
				invoke<CapturePrefs>("shots_prefs"),
				invoke<Preset[]>("shots_presets"),
			]);
			void checkAll(prefs.keys);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function refreshHistory() {
		try {
			entries = await history.loadHistory();
			historyError = null;
		} catch (e) {
			historyError = errText(e);
		}
	}

	/** Save through Rust. A rejection shows Rust's sentence and leaves `prefs` as it was. */
	async function save(next: CapturePrefs) {
		try {
			prefs = await invoke<CapturePrefs>("shots_prefs_set", { prefs: next });
			void checkAll(prefs.keys);
			// A smaller history size prunes entries.
			void refreshHistory();
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function check(field: keyof Keys, stroke: string) {
		if (!stroke) return void (verdicts[field] = undefined);
		try {
			verdicts[field] = await invoke<Verdict>("hotkey_check", {
				stroke,
				screenshotKeysFree: prefs?.systemKeysFree ?? false,
			});
		} catch (e) {
			verdicts[field] = { kind: "refused", reason: errText(e) };
		}
	}

	const checkAll = (keys: Keys) => Promise.all(KEY_FIELDS.map((f) => check(f.field, keys[f.field])));

	/** Ask Rust about the key first: a refused one is explained and never saved. */
	async function commitKey(field: keyof Keys, stroke: string) {
		if (!prefs) return;
		await check(field, stroke);
		const v = verdicts[field];
		if (v?.kind === "refused") {
			toast.error(v.reason);
			return void check(field, prefs.keys[field]);
		}
		await save({ ...prefs, keys: { ...prefs.keys, [field]: stroke } });
	}

	function record(e: KeyboardEvent) {
		if (!recording || !prefs) return;
		e.preventDefault();
		e.stopPropagation();
		const r = recordingResult(e, IS_MAC);
		if (r.kind === "wait") return;
		if (r.kind === "refuse") return void toast.info(r.reason);
		const field = recording;
		recording = null;
		if (r.kind === "stroke") void commitKey(field, r.stroke);
	}

	function applyPicked() {
		const p = presets.find((x) => x.id === picked);
		if (!p || !prefs || !presetAvailable(p, prefs)) return;
		picked = null;
		void save(applyPreset(prefs, p));
	}

	async function run(fn: () => Promise<unknown>) {
		try {
			await fn();
		} catch (e) {
			toast.error(errText(e));
		}
	}

	const grant = () =>
		run(async () => (granted = (await invoke<{ granted: boolean }>("shots_permission_request")).granted));

	const pickFolder = () =>
		run(async () => {
			const folder = await invoke<string | null>("shots_pick_folder");
			if (folder && prefs) await save({ ...prefs, folder });
		});

	/** A whole number in `[min, max]`; an empty or invalid entry changes nothing (the box snaps back). */
	function count(v: number, min: number, max: number, set: (p: CapturePrefs, n: number) => CapturePrefs) {
		if (!prefs) return;
		if (!Number.isFinite(v)) return void (prefs = { ...prefs });
		void save(set(prefs, Math.min(max, Math.max(min, Math.round(v)))));
	}
</script>

<svelte:window onkeydowncapture={record} />

{#snippet keyBody(field: keyof Keys)}
	{@const label = KEY[field].label}
	{@const stroke = prefs?.keys[field] ?? ""}
	<Button
		size="sm"
		variant={recording === field ? "filled" : "outlined"}
		aria-label={m.settings_capture_key_aria({ action: label, key: keyLabel(stroke) })}
		onclick={() => (recording = recording === field ? null : field)}
	>
		{recording === field ? m.settings_capture_key_press() : keyLabel(stroke)}
	</Button>
	{#if stroke && recording !== field}
		<Button
			size="sm"
			variant="ghost"
			aria-label={m.settings_capture_key_clear_aria({ action: label })}
			onclick={() => prefs && save({ ...prefs, keys: { ...prefs.keys, [field]: "" } })}
			>{m.settings_capture_key_clear()}</Button
		>
	{/if}
{/snippet}

{#snippet keyVerdict(field: keyof Keys)}
	{@const v = verdicts[field]}
	{#if v && v.kind !== "ok"}
		<p class="verdict" class:warn={v.kind === "warn"} class:refused={v.kind === "refused"} role="status">
			{verdictText(v)}
		</p>
	{/if}
{/snippet}

<Group title={m.capture_quick_page_capture()} description={m.settings_capture_group_capture_description()}>
	{#if prefs}
		<Row
			id="capture-permission"
			label={m.settings_capture_permission_label()}
			description={m.settings_capture_permission_description()}
		>
			<span class="state" class:ok={granted === true} role="status">
				{granted === null
					? m.settings_capture_permission_checking()
					: granted
						? m.settings_capture_permission_granted()
						: m.settings_capture_permission_not_granted()}
			</span>
			{#if !granted}
				<Button size="sm" variant="filled" onclick={grant}>{m.settings_capture_permission_grant()}</Button>
			{/if}
			<Button size="sm" variant="outlined" onclick={() => run(() => invoke("shots_open_settings"))}
				>{m.settings_capture_permission_open_settings()}</Button
			>
		</Row>
		<Row
			id="capture-enabled"
			label={m.settings_capture_enabled_label()}
			description={m.settings_capture_enabled_description()}
		>
			<Switch
				aria-label={m.settings_capture_enabled_label()}
				checked={prefs.enabled}
				onchange={(enabled) => prefs && save({ ...prefs, enabled })}
			/>
		</Row>
		<Row
			id="capture-folder"
			label={m.settings_capture_folder_label()}
			description={m.settings_capture_folder_description()}
		>
			<span class="path" title={prefs.folder || DEFAULT_DIR}>
				{prefs.folder ? tilde(prefs.folder) : m.settings_capture_folder_default({ folder: DEFAULT_DIR })}
			</span>
			<Button size="sm" variant="outlined" onclick={pickFolder}>{m.settings_capture_folder_choose()}</Button>
			{#if prefs.folder}
				<Button size="sm" variant="ghost" onclick={() => prefs && save({ ...prefs, folder: "" })}
					>{m.settings_capture_folder_reset()}</Button
				>
			{/if}
		</Row>
	{/if}
</Group>

{#if prefs}
	<Group
		title={m.settings_capture_group_hotkeys()}
		description={m.settings_capture_group_hotkeys_description({
			modifiers: list([kbd("cmd"), kbd("ctrl"), kbd("alt")], "disjunction"),
		})}
	>
		<Row
			id="capture-preset"
			label={m.settings_capture_preset_label()}
			description={m.settings_capture_preset_description()}
			stack
		>
			<div class="select">
				<Select
					size="sm"
					aria-label={m.settings_capture_preset_label()}
					value={shownId}
					options={[
						...(activeId === CUSTOM || picked ? [{ value: CUSTOM, label: m.settings_capture_preset_custom() }] : []),
						...presets.map((p) => ({ value: p.id, label: presetLabel(p) })),
					]}
					onchange={(v) => (picked = v === activeId || v === CUSTOM ? null : v)}
				/>
			</div>
			{#snippet below()}
				{#if shownPreset}
					{@const note = presetNote(shownPreset)}
					{#if note}
						<div class="callout" role="note">{note}</div>
					{/if}
				{/if}
				{#if picked}
					<div class="actions">
						<Button
							size="sm"
							variant="filled"
							disabled={!pickedAvailable}
							title={pickedAvailable ? undefined : m.settings_capture_preset_apply_blocked()}
							onclick={applyPicked}>{m.settings_capture_preset_apply()}</Button
						>
						<Button size="sm" variant="ghost" onclick={() => (picked = null)}>{m.common_action_cancel()}</Button>
					</div>
				{/if}
			{/snippet}
		</Row>
		<Row
			id="capture-system-keys"
			label={m.settings_capture_system_keys_label()}
			description={m.settings_capture_system_keys_description(systemKeyArgs())}
			stack
		>
			<Button size="sm" variant="outlined" onclick={() => run(() => invoke("shots_open_shortcuts_settings"))}
				>{m.settings_capture_system_keys_open()}</Button
			>
			<label class="inline">
				<Switch
					aria-label={m.settings_capture_system_keys_off_aria()}
					checked={prefs.systemKeysFree}
					onchange={(systemKeysFree) => prefs && save({ ...prefs, systemKeysFree })}
				/>
				<span>{m.settings_capture_system_keys_off_label()}</span>
			</label>
			{#snippet below()}
				{#if usedSystem.length && !prefs?.systemKeysFree}
					<div class="clash" role="alert">{systemKeysClash(usedSystem)}</div>
				{:else if prefs?.systemKeysFree}
					<div class="callout" role="note">
						{m.settings_capture_system_keys_accepted({ range: systemKeyArgs().range })}
					</div>
				{/if}
			{/snippet}
		</Row>
		<Row id="capture-key-all-in-one" label={KEY.allInOne.label} description={KEY.allInOne.description}>
			{@render keyBody("allInOne")}
			{#snippet below()}{@render keyVerdict("allInOne")}{/snippet}
		</Row>
		<Row id="capture-key-region" label={KEY.region.label} description={KEY.region.description}>
			{@render keyBody("region")}
			{#snippet below()}{@render keyVerdict("region")}{/snippet}
		</Row>
		<Row id="capture-key-previous" label={KEY.previous.label} description={KEY.previous.description}>
			{@render keyBody("previous")}
			{#snippet below()}{@render keyVerdict("previous")}{/snippet}
		</Row>
		<Row id="capture-key-window" label={KEY.window.label} description={KEY.window.description}>
			{@render keyBody("window")}
			{#snippet below()}{@render keyVerdict("window")}{/snippet}
		</Row>
		<Row id="capture-key-fullscreen" label={KEY.fullscreen.label} description={KEY.fullscreen.description}>
			{@render keyBody("fullscreen")}
			{#snippet below()}{@render keyVerdict("fullscreen")}{/snippet}
		</Row>
		<Row id="capture-key-ocr" label={KEY.ocr.label} description={KEY.ocr.description}>
			{@render keyBody("ocr")}
			{#snippet below()}{@render keyVerdict("ocr")}{/snippet}
		</Row>
		<Row id="capture-key-record" label={KEY.record.label} description={KEY.record.description}>
			{@render keyBody("record")}
			{#snippet below()}{@render keyVerdict("record")}{/snippet}
		</Row>
		<Row id="capture-key-gif" label={KEY.gif.label} description={KEY.gif.description}>
			{@render keyBody("gif")}
			{#snippet below()}{@render keyVerdict("gif")}{/snippet}
		</Row>
		{#if clash}
			<div class="clash" role="alert">{clash}</div>
		{/if}
	</Group>

	<Group title={m.settings_capture_group_screenshots()}>
		<Row
			id="capture-on-release"
			label={m.settings_capture_on_release_label()}
			description={m.settings_capture_on_release_description()}
		>
			<Switch
				aria-label={m.settings_capture_on_release_label()}
				checked={prefs.still.captureOnRelease}
				onchange={(captureOnRelease) =>
					prefs && save({ ...prefs, still: { ...prefs.still, captureOnRelease } })}
			/>
		</Row>
		<Row
			id="capture-timer"
			label={m.settings_capture_timer_label()}
			description={m.settings_capture_timer_description()}
		>
			<div class="select">
				<Select
					size="sm"
					aria-label={m.settings_capture_timer_label()}
					value={String(prefs.still.timerSecs)}
					options={timerOptions()}
					onchange={(v) => prefs && save({ ...prefs, still: { ...prefs.still, timerSecs: Number(v) } })}
				/>
			</div>
		</Row>
		<Row
			id="capture-scale"
			label={m.settings_capture_scale_label()}
			description={m.settings_capture_scale_description()}
		>
			<div class="select">
				<Select
					size="sm"
					aria-label={m.settings_capture_scale_label()}
					value={prefs.still.scale}
					options={scaleOptions()}
					onchange={(v) =>
						prefs &&
						save({ ...prefs, still: { ...prefs.still, scale: v as CapturePrefs["still"]["scale"] } })}
				/>
			</div>
		</Row>
		<Row
			id="capture-hide-icons"
			label={m.settings_capture_hide_icons_label()}
			description={m.settings_capture_hide_icons_description()}
		>
			<Switch
				aria-label={m.settings_capture_hide_icons_label()}
				checked={prefs.still.hideIcons}
				onchange={(hideIcons) => prefs && save({ ...prefs, still: { ...prefs.still, hideIcons } })}
			/>
		</Row>
		<Row
			id="capture-window-shadow"
			label={m.settings_capture_window_shadow_label()}
			description={m.settings_capture_window_shadow_description()}
		>
			<Switch
				aria-label={m.settings_capture_window_shadow_label()}
				checked={prefs.still.windowShadow}
				onchange={(windowShadow) => prefs && save({ ...prefs, still: { ...prefs.still, windowShadow } })}
			/>
		</Row>
		<Row
			id="capture-window-padding"
			label={m.settings_capture_window_background_label()}
			description={m.settings_capture_window_background_description()}
		>
			<Switch
				aria-label={m.settings_capture_window_background_label()}
				checked={prefs.still.windowPadding}
				onchange={(windowPadding) => prefs && save({ ...prefs, still: { ...prefs.still, windowPadding } })}
			/>
		</Row>
	</Group>

	<Group title={m.capture_quick_title_recording()}>
		<Row
			id="capture-quality"
			label={m.settings_capture_quality_label()}
			description={m.settings_capture_quality_description()}
		>
			<div class="select">
				<Select
					size="sm"
					aria-label={m.settings_capture_quality_label()}
					value={prefs.recording.quality}
					options={qualityOptions()}
					onchange={(v) =>
						prefs &&
						save({
							...prefs,
							recording: { ...prefs.recording, quality: v as CapturePrefs["recording"]["quality"] },
						})}
				/>
			</div>
		</Row>
		<Row
			id="capture-clicks"
			label={m.settings_capture_clicks_label()}
			description={m.settings_capture_clicks_description()}
		>
			<Switch
				aria-label={m.settings_capture_clicks_label()}
				checked={prefs.recording.clicks}
				onchange={(clicks) => prefs && save({ ...prefs, recording: { ...prefs.recording, clicks } })}
			/>
		</Row>
		<Row
			id="capture-keystrokes"
			label={m.settings_capture_keystrokes_label()}
			description={m.settings_capture_keystrokes_description()}
		>
			<Switch
				aria-label={m.settings_capture_keystrokes_label()}
				checked={prefs.recording.keystrokes}
				onchange={(keystrokes) => prefs && save({ ...prefs, recording: { ...prefs.recording, keystrokes } })}
			/>
		</Row>
		<Row
			id="capture-system-audio"
			label={m.settings_capture_system_audio_label()}
			description={m.settings_capture_system_audio_description()}
			stack={!!caps?.reason}
		>
			<Switch
				aria-label={m.settings_capture_system_audio_label()}
				checked={prefs.recording.systemAudio && !!caps?.systemAudio}
				disabled={!caps?.systemAudio}
				onchange={(systemAudio) =>
					prefs && save({ ...prefs, recording: { ...prefs.recording, systemAudio } })}
			/>
			{#snippet below()}
				{#if caps?.reason && !caps.systemAudio}<p class="verdict warn" role="status">{caps.reason}</p>{/if}
			{/snippet}
		</Row>
		<Row
			id="capture-microphone"
			label={m.settings_capture_microphone_label()}
			description={m.settings_capture_microphone_description()}
			stack={!!caps?.reason}
		>
			<Switch
				aria-label={m.settings_capture_microphone_label()}
				checked={prefs.recording.microphone && !!caps?.microphone}
				disabled={!caps?.microphone}
				onchange={(microphone) => prefs && save({ ...prefs, recording: { ...prefs.recording, microphone } })}
			/>
			{#snippet below()}
				{#if caps?.reason && !caps.microphone}<p class="verdict warn" role="status">{caps.reason}</p>{/if}
			{/snippet}
		</Row>
	</Group>

	<Group title={m.settings_capture_group_after()}>
		<Row
			id="capture-copy"
			label={m.capture_editor_page_copy_to_clipboard()}
			description={m.settings_capture_copy_description()}
		>
			<Switch
				aria-label={m.capture_editor_page_copy_to_clipboard()}
				checked={prefs.after.copy}
				onchange={(copy) => prefs && save({ ...prefs, after: { ...prefs.after, copy } })}
			/>
		</Row>
		<Row
			id="capture-quick-access"
			label={m.settings_capture_quick_access_label()}
			description={m.settings_capture_quick_access_description()}
		>
			<Switch
				aria-label={m.settings_capture_quick_access_label()}
				checked={prefs.after.quickAccess}
				onchange={(quickAccess) => prefs && save({ ...prefs, after: { ...prefs.after, quickAccess } })}
			/>
		</Row>
		<Row
			id="capture-dismiss"
			label={m.settings_capture_dismiss_label()}
			description={m.settings_capture_dismiss_description()}
		>
			<NumInput
				label={m.settings_capture_dismiss_seconds_label()}
				unit="s"
				value={prefs.after.dismissSecs}
				onchange={(n) => count(n, 0, 60, (p, dismissSecs) => ({ ...p, after: { ...p.after, dismissSecs } }))}
			/>
		</Row>
		<Row
			id="capture-corner"
			label={m.settings_capture_corner_label()}
			description={m.settings_capture_corner_description()}
		>
			<div class="select">
				<Select
					size="sm"
					aria-label={m.settings_capture_corner_label()}
					value={prefs.quick.position}
					options={cornerOptions()}
					onchange={(v) =>
						prefs &&
						save({ ...prefs, quick: { ...prefs.quick, position: v as CapturePrefs["quick"]["position"] } })}
				/>
			</div>
		</Row>
		<Row
			id="capture-stack"
			label={m.settings_capture_stack_label()}
			description={m.settings_capture_stack_description()}
		>
			<NumInput
				label={m.settings_capture_stack_label()}
				unit={m.settings_capture_stack_unit()}
				value={prefs.quick.stack}
				onchange={(n) => count(n, 1, 8, (p, stack) => ({ ...p, quick: { ...p.quick, stack } }))}
			/>
		</Row>
		<Row
			id="capture-sound"
			label={m.settings_capture_sound_label()}
			description={m.settings_capture_sound_description()}
		>
			<Switch
				aria-label={m.settings_capture_sound_label()}
				checked={prefs.after.sound}
				onchange={(sound) => prefs && save({ ...prefs, after: { ...prefs.after, sound } })}
			/>
		</Row>
		<Row
			id="capture-highlight"
			label={m.settings_capture_highlight_label()}
			description={m.settings_capture_highlight_description()}
		>
			<Switch
				aria-label={m.settings_capture_highlight_label()}
				checked={prefs.highlightCursor}
				onchange={(highlightCursor) => prefs && save({ ...prefs, highlightCursor })}
			/>
		</Row>
	</Group>

	<Group title={m.settings_capture_group_history()} description={m.settings_capture_group_history_description()}>
		<Row
			id="capture-history-size"
			label={m.settings_capture_history_size_label()}
			description={m.settings_capture_history_size_description()}
		>
			<NumInput
				label={m.settings_capture_history_size_label()}
				unit={m.settings_capture_history_unit()}
				value={prefs.history}
				onchange={(n) => count(n, 1, 500, (p, history) => ({ ...p, history }))}
			/>
		</Row>
		<Row
			id="capture-history"
			label={m.settings_capture_recent_label()}
			description={m.settings_capture_recent_description()}
			stack
		>
			{#snippet below()}
				{#if historyError}
					<p class="verdict refused" role="alert">{historyError}</p>
				{:else if !groups.length}
					<p class="empty">{m.settings_capture_recent_empty()}</p>
				{/if}
				{#each groups as g (g.title)}
					<section class="hgroup" aria-label={g.title}>
						<h4>{g.title}</h4>
						<ul>
							{#each g.entries as e (e.path)}
								{@const l = historyLabel(e)}
								{@const Icon = ICON[e.kind]}
								<li>
									<span class="kind" title={l.kind}><Icon size={14} aria-label={l.kind} /></span>
									<span class="meta">
										<span class="name" title={e.path}>{l.name}</span>
										<span class="sub">{l.detail} · {dateTime(e.at)}</span>
									</span>
									<span class="btns">
										<Button size="sm" variant="ghost" onclick={() => run(() => history.open(e.path))}
											>{m.common_action_open()}</Button
										>
										<Button size="sm" variant="ghost" onclick={() => run(() => history.reveal(e.path))}
											>{revealLabel()}</Button
										>
										<Button
											size="sm"
											variant="ghost"
											title={m.settings_capture_recent_forget()}
											onclick={() => run(async () => (await history.remove(e.path), await refreshHistory()))}
											>{m.settings_capture_recent_remove()}</Button
										>
									</span>
								</li>
							{/each}
						</ul>
					</section>
				{/each}
			{/snippet}
		</Row>
	</Group>
{/if}

<style>
	.state {
		font-size: 12px;
		color: var(--ca-warning-main, #b45309);
	}
	.state.ok {
		color: var(--ca-success-main, #15803d);
	}
	.select {
		min-width: 190px;
	}
	.path {
		max-width: 260px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.inline {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
	}
	.callout {
		margin-top: 4px;
		padding: 9px 12px;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--ca-warning-main, #b45309) 45%, transparent);
		background: color-mix(in srgb, var(--ca-warning-main, #b45309) 10%, transparent);
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-primary);
	}
	.actions {
		display: flex;
		gap: 8px;
		margin-top: 8px;
	}
	.verdict {
		margin: 0;
		font-size: 12px;
		line-height: 1.4;
	}
	.verdict.warn {
		color: var(--ca-warning-main, #b45309);
	}
	.verdict.refused {
		color: var(--ca-error-main, #b91c1c);
	}
	.clash {
		padding: 10px 16px;
		font-size: 12px;
		color: var(--ca-error-main, #b91c1c);
	}
	.empty {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.hgroup h4 {
		margin: 10px 0 4px;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 0;
		min-width: 0;
	}
	.kind {
		display: grid;
		place-items: center;
		flex-shrink: 0;
		color: var(--ca-text-secondary);
	}
	.meta {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}
	.name {
		font-size: 13px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.sub {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.btns {
		display: flex;
		gap: 2px;
		flex-shrink: 0;
	}
</style>
