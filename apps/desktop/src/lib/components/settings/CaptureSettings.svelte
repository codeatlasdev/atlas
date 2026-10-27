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
	import { IS_MAC } from "#lib/keys.ts";
	import Group from "#lib/settings/Group.svelte";
	import NumInput from "#lib/settings/NumInput.svelte";
	import Row from "#lib/settings/Row.svelte";
	import * as history from "#lib/shots/history.ts";
	import {
		CORNER_OPTIONS,
		KEY_FIELDS,
		QUALITY_OPTIONS,
		SCALE_OPTIONS,
		TIMER_OPTIONS,
		activePresetId,
		applyPreset,
		clashes,
		historyGroups,
		historyLabel,
		keyLabel,
		presetAvailable,
		recordingResult,
		systemKeysUsed,
		verdictText,
	} from "#lib/shots/settings.ts";
	import type { CapturePrefs, Entry, Keys, Preset, RecordCapabilities, Verdict } from "#lib/shots/types.ts";

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

	const when = (at: number) =>
		new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
</script>

<svelte:window onkeydowncapture={record} />

{#snippet keyBody(field: keyof Keys)}
	{@const label = KEY[field].label}
	{@const stroke = prefs?.keys[field] ?? ""}
	<Button
		size="sm"
		variant={recording === field ? "filled" : "outlined"}
		aria-label={`${label} hotkey: ${keyLabel(stroke)}. Click to change`}
		onclick={() => (recording = recording === field ? null : field)}
	>
		{recording === field ? "Press a key…" : keyLabel(stroke)}
	</Button>
	{#if stroke && recording !== field}
		<Button
			size="sm"
			variant="ghost"
			aria-label={`Clear the ${label} hotkey`}
			onclick={() => prefs && save({ ...prefs, keys: { ...prefs.keys, [field]: "" } })}>Clear</Button
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

<Group
	title="Capture"
	description="Take screenshots and record the screen from a hotkey, from any app. Files go to the folder below."
>
	{#if prefs}
		<Row
			id="capture-permission"
			label="Screen Recording access"
			description="Screen Recording lets Atlas see the pixels of other apps. It is only used when you press a capture hotkey; nothing is recorded in the background. After granting it, macOS asks you to restart Atlas."
		>
			<span class="state" class:ok={granted === true} role="status">
				{granted === null ? "Checking…" : granted ? "Granted" : "Not granted"}
			</span>
			{#if !granted}
				<Button size="sm" variant="filled" onclick={grant}>Grant access</Button>
			{/if}
			<Button size="sm" variant="outlined" onclick={() => run(() => invoke("shots_open_settings"))}
				>Open System Settings</Button
			>
		</Row>
		<Row
			id="capture-enabled"
			label="Capture hotkeys"
			description="Turn the capture hotkeys on or off. Off gives the keys back to other apps."
		>
			<Switch
				aria-label="Capture hotkeys"
				checked={prefs.enabled}
				onchange={(enabled) => prefs && save({ ...prefs, enabled })}
			/>
		</Row>
		<Row id="capture-folder" label="Save to" description="Where captures are saved.">
			<span class="path" title={prefs.folder || DEFAULT_DIR}>
				{prefs.folder ? tilde(prefs.folder) : `${DEFAULT_DIR} (default)`}
			</span>
			<Button size="sm" variant="outlined" onclick={pickFolder}>Choose…</Button>
			{#if prefs.folder}
				<Button size="sm" variant="ghost" onclick={() => prefs && save({ ...prefs, folder: "" })}
					>Reset</Button
				>
			{/if}
		</Row>
	{/if}
</Group>

{#if prefs}
	<Group
		title="Hotkeys"
		description="Click a key, then press the new one. Esc cancels. A key must include ⌘, ⌃ or ⌥ so it does not take that key from every app."
	>
		<Row
			id="capture-preset"
			label="Key set"
			description="Start from a ready-made set of keys. Picking one shows what it changes; nothing is saved until you apply it."
			stack
		>
			<div class="select">
				<Select
					size="sm"
					aria-label="Key set"
					value={shownId}
					options={[
						...(activeId === CUSTOM || picked ? [{ value: CUSTOM, label: "Custom" }] : []),
						...presets.map((p) => ({ value: p.id, label: p.label })),
					]}
					onchange={(v) => (picked = v === activeId || v === CUSTOM ? null : v)}
				/>
			</div>
			{#snippet below()}
				{#if shownPreset?.note}
					<div class="callout" role="note">{shownPreset.note}</div>
				{/if}
				{#if picked}
					<div class="actions">
						<Button
							size="sm"
							variant="filled"
							disabled={!pickedAvailable}
							title={pickedAvailable ? undefined : "Turn off macOS's own screenshot shortcuts first (below)"}
							onclick={applyPicked}>Apply these keys</Button
						>
						<Button size="sm" variant="ghost" onclick={() => (picked = null)}>Cancel</Button>
					</div>
				{/if}
			{/snippet}
		</Row>
		<Row
			id="capture-system-keys"
			label="macOS screenshot shortcuts"
			description="macOS answers ⇧⌘3, ⇧⌘4, ⇧⌘5 and ⇧⌘6 before any app, so Atlas can only use them once you turn them off yourself in System Settings ▸ Keyboard ▸ Keyboard Shortcuts ▸ Screenshots. Atlas never changes that setting for you."
			stack
		>
			<Button size="sm" variant="outlined" onclick={() => run(() => invoke("shots_open_shortcuts_settings"))}
				>Open Keyboard Shortcuts</Button
			>
			<label class="inline">
				<Switch
					aria-label="I turned the macOS screenshot shortcuts off"
					checked={prefs.systemKeysFree}
					onchange={(systemKeysFree) => prefs && save({ ...prefs, systemKeysFree })}
				/>
				<span>I turned them off</span>
			</label>
			{#snippet below()}
				{#if usedSystem.length && !prefs?.systemKeysFree}
					<div class="clash" role="alert">
						{usedSystem.join(", ")} still belong to macOS. Turn them off in System Settings, tick the box above,
						or pick other keys.
					</div>
				{:else if prefs?.systemKeysFree}
					<div class="callout" role="note">
						Atlas now accepts ⇧⌘3–6. If you turn the macOS ones back on, these keys stop working here.
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

	<Group title="Screenshots">
		<Row
			id="capture-on-release"
			label="Capture when the mouse is released"
			description="Take the shot as soon as you let go of the selection; turn off to adjust it first and confirm with Return."
		>
			<Switch
				aria-label="Capture when the mouse is released"
				checked={prefs.still.captureOnRelease}
				onchange={(captureOnRelease) =>
					prefs && save({ ...prefs, still: { ...prefs.still, captureOnRelease } })}
			/>
		</Row>
		<Row
			id="capture-timer"
			label="Self-timer"
			description="Wait this long after you pick before a screenshot is taken. Recordings are not delayed."
		>
			<div class="select">
				<Select
					size="sm"
					aria-label="Self-timer"
					value={String(prefs.still.timerSecs)}
					options={[...TIMER_OPTIONS]}
					onchange={(v) => prefs && save({ ...prefs, still: { ...prefs.still, timerSecs: Number(v) } })}
				/>
			</div>
		</Row>
		<Row
			id="capture-scale"
			label="Resolution"
			description="Retina saves the display's own pixels; Standard saves one pixel per point, a smaller file."
		>
			<div class="select">
				<Select
					size="sm"
					aria-label="Resolution"
					value={prefs.still.scale}
					options={[...SCALE_OPTIONS]}
					onchange={(v) =>
						prefs &&
						save({ ...prefs, still: { ...prefs.still, scale: v as CapturePrefs["still"]["scale"] } })}
				/>
			</div>
		</Row>
		<Row
			id="capture-hide-icons"
			label="Hide desktop icons"
			description="Hide the icons on the desktop while a screenshot or recording is taken."
		>
			<Switch
				aria-label="Hide desktop icons"
				checked={prefs.still.hideIcons}
				onchange={(hideIcons) => prefs && save({ ...prefs, still: { ...prefs.still, hideIcons } })}
			/>
		</Row>
		<Row
			id="capture-window-shadow"
			label="Window shadow"
			description="A window capture keeps its drop shadow, on a transparent background."
		>
			<Switch
				aria-label="Window shadow"
				checked={prefs.still.windowShadow}
				onchange={(windowShadow) => prefs && save({ ...prefs, still: { ...prefs.still, windowShadow } })}
			/>
		</Row>
		<Row
			id="capture-window-padding"
			label="Window background"
			description="A window capture sits on a padded background instead of a shadow. It wins over the shadow when both are on."
		>
			<Switch
				aria-label="Window background"
				checked={prefs.still.windowPadding}
				onchange={(windowPadding) => prefs && save({ ...prefs, still: { ...prefs.still, windowPadding } })}
			/>
		</Row>
	</Group>

	<Group title="Recording">
		<Row
			id="capture-quality"
			label="Video quality"
			description="Lower qualities record fewer pixels, so the file is smaller. GIFs are not affected."
		>
			<div class="select">
				<Select
					size="sm"
					aria-label="Video quality"
					value={prefs.recording.quality}
					options={[...QUALITY_OPTIONS]}
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
			label="Show clicks"
			description="A ring on each click in screen recordings and GIFs."
		>
			<Switch
				aria-label="Show clicks"
				checked={prefs.recording.clicks}
				onchange={(clicks) => prefs && save({ ...prefs, recording: { ...prefs.recording, clicks } })}
			/>
		</Row>
		<Row
			id="capture-keystrokes"
			label="Show keystrokes"
			description="A pill with the keys you press at the bottom of the recorded area. macOS asks for Input Monitoring the first time."
		>
			<Switch
				aria-label="Show keystrokes"
				checked={prefs.recording.keystrokes}
				onchange={(keystrokes) => prefs && save({ ...prefs, recording: { ...prefs.recording, keystrokes } })}
			/>
		</Row>
		<Row
			id="capture-system-audio"
			label="Record system sound"
			description="The sound your Mac plays goes into the video. Not in GIFs."
			stack={!!caps?.reason}
		>
			<Switch
				aria-label="Record system sound"
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
			label="Record the microphone"
			description="Your voice goes into the video. macOS asks for microphone access the first time. Not in GIFs."
			stack={!!caps?.reason}
		>
			<Switch
				aria-label="Record the microphone"
				checked={prefs.recording.microphone && !!caps?.microphone}
				disabled={!caps?.microphone}
				onchange={(microphone) => prefs && save({ ...prefs, recording: { ...prefs.recording, microphone } })}
			/>
			{#snippet below()}
				{#if caps?.reason && !caps.microphone}<p class="verdict warn" role="status">{caps.reason}</p>{/if}
			{/snippet}
		</Row>
	</Group>

	<Group title="After a capture">
		<Row
			id="capture-copy"
			label="Copy to clipboard"
			description="Put every capture on the clipboard as soon as it is taken: screenshots as an image, recordings and GIFs as the file."
		>
			<Switch
				aria-label="Copy to clipboard"
				checked={prefs.after.copy}
				onchange={(copy) => prefs && save({ ...prefs, after: { ...prefs.after, copy } })}
			/>
		</Row>
		<Row
			id="capture-quick-access"
			label="Show the quick access overlay"
			description="A small card with the capture: copy, save as, annotate, send to an agent, or drag it into another app."
		>
			<Switch
				aria-label="Show the quick access overlay"
				checked={prefs.after.quickAccess}
				onchange={(quickAccess) => prefs && save({ ...prefs, after: { ...prefs.after, quickAccess } })}
			/>
		</Row>
		<Row
			id="capture-dismiss"
			label="Auto-dismiss the overlay"
			description="Seconds before the card goes away by itself, 0 to 60. 0 keeps it until you close it."
		>
			<NumInput
				label="Auto-dismiss seconds"
				unit="s"
				value={prefs.after.dismissSecs}
				onchange={(n) => count(n, 0, 60, (p, dismissSecs) => ({ ...p, after: { ...p.after, dismissSecs } }))}
			/>
		</Row>
		<Row
			id="capture-corner"
			label="Overlay corner"
			description="The corner of the display under the pointer where the quick access cards stack."
		>
			<div class="select">
				<Select
					size="sm"
					aria-label="Overlay corner"
					value={prefs.quick.position}
					options={[...CORNER_OPTIONS]}
					onchange={(v) =>
						prefs &&
						save({ ...prefs, quick: { ...prefs.quick, position: v as CapturePrefs["quick"]["position"] } })}
				/>
			</div>
		</Row>
		<Row
			id="capture-stack"
			label="Cards in the stack"
			description="How many quick access cards stack in the corner before the oldest goes, 1 to 8."
		>
			<NumInput
				label="Cards in the stack"
				unit="cards"
				value={prefs.quick.stack}
				onchange={(n) => count(n, 1, 8, (p, stack) => ({ ...p, quick: { ...p.quick, stack } }))}
			/>
		</Row>
		<Row id="capture-sound" label="Play sound" description="A shutter sound when a screenshot is taken.">
			<Switch
				aria-label="Play sound"
				checked={prefs.after.sound}
				onchange={(sound) => prefs && save({ ...prefs, after: { ...prefs.after, sound } })}
			/>
		</Row>
		<Row
			id="capture-highlight"
			label="Highlight the cursor and clicks"
			description="Show the pointer and a ring on each click in screen recordings and GIFs."
		>
			<Switch
				aria-label="Highlight the cursor and clicks"
				checked={prefs.highlightCursor}
				onchange={(highlightCursor) => prefs && save({ ...prefs, highlightCursor })}
			/>
		</Row>
	</Group>

	<Group
		title="History"
		description="Your recent captures. Removing one only forgets it here: the file is never deleted from your disk."
	>
		<Row
			id="capture-history-size"
			label="History size"
			description="How many captures to remember, 1 to 500. Older ones drop off the list; their files stay."
		>
			<NumInput
				label="History size"
				unit="captures"
				value={prefs.history}
				onchange={(n) => count(n, 1, 500, (p, history) => ({ ...p, history }))}
			/>
		</Row>
		<Row
			id="capture-history"
			label="Recent captures"
			description="Open, reveal or remove from the list your recent captures. Removing never deletes the file."
			stack
		>
			{#snippet below()}
				{#if historyError}
					<p class="verdict refused" role="alert">{historyError}</p>
				{:else if !groups.length}
					<p class="empty">Nothing captured yet.</p>
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
										<span class="sub">{l.detail} · {when(e.at)}</span>
									</span>
									<span class="btns">
										<Button size="sm" variant="ghost" onclick={() => run(() => history.open(e.path))}
											>Open</Button
										>
										<Button size="sm" variant="ghost" onclick={() => run(() => history.reveal(e.path))}
											>Reveal</Button
										>
										<Button
											size="sm"
											variant="ghost"
											title="Forget this capture. The file stays on your disk."
											onclick={() => run(async () => (await history.remove(e.path), await refreshHistory()))}
											>Remove from list</Button
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
