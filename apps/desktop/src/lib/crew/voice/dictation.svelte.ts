/**
 * Dictation in the message box: the state of one microphone session and what it does to the draft.
 *
 * The audio is recorded by the shell (`crates/atlas-desktop/src/voice`), uploaded here in pieces to the crew server
 * (`crewmate_voice_put`), which transcribes it (`crewmate_transcribe`). While the person talks, the audio so far is
 * transcribed every couple of seconds for a dimmed preview (`partial: true`); when they stop, the whole recording is
 * transcribed once and that text goes into the draft, at the caret or over the selection. Rules that do not bend:
 * nothing is ever sent by this class (the person presses Send), the engine in use is always named, a failed engine
 * is an error with no other engine behind it, and Escape takes the dictated text back out.
 *
 * Everything outside (the shell, hostd, the clock, the catalog) is injected, so the whole session is tested with
 * fakes. Pure runes state: no DOM here; the composer owns the textarea and hands over a `Draft`.
 */
import { b64encode } from "#lib/b64.ts";
import { CHUNK, type HostCall } from "#lib/crew/attachments.ts";
import { m } from "#lib/i18n/m.ts";
import { insertDictation } from "./insert.ts";
import { parseTranscript, type VoiceStatus } from "./wire.ts";

/** How the microphone key works: hold while talking, or press to start and press to stop. */
export type KeyMode = "hold" | "tap";

/** A press shorter than this does not end the dictation on release: it latches (the next press stops). */
export const LATCH_MS = 350;
/** The preview is refreshed at most this often. */
export const PARTIAL_EVERY_MS = 2000;

/** Why the shell stopped by itself, or at the page's request (`voice::recorder::Reason`). */
export type Reason = "requested" | "cap" | "background" | "device_lost";

/** What the shell reports about a finished recording (`voice::recorder::Take`). */
export interface Take {
	reason: Reason;
	bytes: number;
	seconds: number;
}

/** A start the shell refused (`voice::recorder::StartError`, tagged by `code`). */
export interface StartFailure {
	code: "busy" | "denied" | "no_device" | "unavailable" | "failed";
	detail?: string;
}

/** The shell's side of dictation (Tauri commands and events). */
export interface Native {
	/** Opens the microphone; rejects with a `StartFailure`. */
	start(): Promise<void>;
	stop(): Promise<Take | null>;
	cancel(): Promise<void>;
	/** A piece of the WAV from `offset`; `total` is the file's size so far, `live` whether it can still grow. */
	read(offset: number, max: number): Promise<{ data: Uint8Array; total: number; live: boolean }>;
	openSettings(): Promise<void>;
	onLevel(cb: (level: number) => void): Promise<() => void>;
	onStopped(cb: (take: Take) => void): Promise<() => void>;
}

/** The draft in the textarea at this moment. */
export interface Draft {
	value: string;
	start: number;
	end: number;
}

/** The composer's side: read the draft and caret, write a new draft and place the caret. */
export interface Target {
	read(): Draft;
	write(value: string, cursor: number): void;
}

export interface Deps {
	native: Native;
	/** A host call already routed to the selected crewmate (`host`, `id`). */
	call: HostCall;
	/** The selected crew server's voice status as last loaded; null while unknown or not supported. */
	status(): VoiceStatus | null;
	/** The sentence of a coded hostd refusal, in the app's language. */
	coded(code: string, args: Record<string, unknown>): Promise<string>;
	/** Text of a rejection of a host call (a coded refusal is already in the app's language). */
	errorText(e: unknown): string;
	mode(): KeyMode;
	now(): number;
	/** Runs `fn` after `ms`; the returned function cancels it. */
	later(fn: () => void, ms: number): () => void;
}

export type Phase = "idle" | "starting" | "listening" | "transcribing" | "error";

/** What an error offers: nothing, the system's microphone settings, or another try with the audio kept. */
export interface Failure {
	text: string;
	action: "none" | "settings" | "retry";
}

/** Refusals of hostd that the same recording will meet again: the person must change something first. */
const SETTLED: Record<string, true> = {
	"crew.voice.off": true,
	"crew.voice.not_installed": true,
	"crew.voice.forbidden": true,
	"crew.voice.too_long": true,
	"crew.voice.bad_audio": true,
};

export class Dictation {
	phase = $state<Phase>("idle");
	/** 0..1, the input level (20 a second). */
	level = $state(0);
	/** Milliseconds of sound so far. */
	elapsedMs = $state(0);
	/** The live preview text (dimmed); empty until the first partial. */
	partial = $state("");
	/** The engine's label, shown the whole time the microphone is on. */
	engine = $state("");
	error = $state<Failure | null>(null);
	/** What a screen reader hears: start, stop, auto-stop, insertion. */
	announce = $state("");
	/** Any part of the draft was dictated: the task goes out with `dictated: true`. */
	dictated = $state(false);

	private target: Target | null = null;
	private pressedAt = 0;
	/** The press was short: the dictation stays on until the next press. */
	private latched = false;
	/** The key came up while the microphone was still opening. */
	private release: "latch" | "stop" | null = null;
	/** Bumped by anything that ends the session, so a late answer knows it is stale. */
	private run = 0;
	private upload = "";
	private sent = 0;
	private cancelTick: (() => void) | null = null;
	private inflight: Promise<void> = Promise.resolve();
	private undo: { before: string; after: string; cursor: number; dictated: boolean } | null = null;
	private off: (() => void)[] = [];
	/** Counts `attach`/`dispose`: a listener that arrives after its composer is gone is released at once. */
	private mounts = 0;

	constructor(private readonly deps: Deps) {}

	/** The microphone is on or about to be, or its audio is being turned into text. */
	get busy(): boolean {
		return this.phase === "starting" || this.phase === "listening" || this.phase === "transcribing";
	}

	get listening(): boolean {
		return this.phase === "listening";
	}

	/** Listen to the shell. Call once when the composer mounts; `dispose` undoes it. */
	async attach(): Promise<void> {
		const mine = ++this.mounts;
		const levels = await this.deps.native.onLevel((l) => {
			if (this.phase !== "listening") return;
			this.level = l;
			this.elapsedMs += 50;
		});
		const stopped = await this.deps.native.onStopped((take) => {
			// The page asked for this stop itself: `finish` already has it.
			if (take.reason !== "requested" && this.phase === "listening") void this.finish(take);
		});
		if (mine !== this.mounts) {
			levels();
			stopped();
			return;
		}
		this.off.push(levels, stopped);
	}

	/** The composer is going away: release the microphone, drop the audio, stop listening to the shell. */
	dispose(): void {
		this.mounts++;
		this.off.forEach((f) => f());
		this.off = [];
		this.abandon();
	}

	// ─── the key and the button ────────────────────────────────────────────

	/** The key or button went down. */
	press(target: Target): void {
		this.target = target;
		if (this.phase === "listening" || this.phase === "starting") {
			// A press while it is on ends it: tap mode's second tap, or hold mode's tap after a latch.
			if (this.phase === "listening" && (this.deps.mode() === "tap" || this.latched)) void this.stop();
			return;
		}
		if (this.phase === "transcribing") return;
		this.pressedAt = this.deps.now();
		this.latched = false;
		this.release = null;
		void this.begin();
	}

	/** The button was activated from the keyboard (no press to hold): starts and stays on, or stops. */
	toggle(target: Target): void {
		this.target = target;
		if (this.phase === "listening") void this.stop();
		else if (this.phase === "idle" || this.phase === "error") {
			this.pressedAt = this.deps.now();
			this.latched = true;
			this.release = null;
			void this.begin();
		}
	}

	/** The key or button came up (hold mode only: tap mode ignores it). */
	letGo(): void {
		if (this.deps.mode() !== "hold") return;
		const short = this.deps.now() - this.pressedAt < LATCH_MS;
		if (this.phase === "starting") this.release = short ? "latch" : "stop";
		else if (this.phase === "listening" && !this.latched) {
			if (short) this.latched = true;
			else void this.stop();
		}
	}

	/** Escape: stop the microphone and forget what was said. */
	cancel(): void {
		if (!this.busy) return;
		this.abandon();
		this.announce = m.crew_voice_announce_cancelled();
	}

	// ─── the session ───────────────────────────────────────────────────────

	private async begin(): Promise<void> {
		const run = ++this.run;
		this.error = null;
		this.partial = "";
		this.level = 0;
		this.elapsedMs = 0;
		this.upload = "";
		this.sent = 0;
		const status = this.deps.status();
		if (!status?.enabled) return this.refuse(run, "crew.voice.off", {});
		if (!status.engine) return this.refuse(run, "crew.voice.not_installed", {});
		if (!status.engine.ready) {
			return this.refuse(run, "crew.voice.engine_failed", {
				engine: status.engine.label,
				detail: status.engine.detail ?? "",
			});
		}
		this.engine = status.engine.label;
		this.phase = "starting";
		try {
			await this.deps.native.start();
		} catch (e) {
			if (run === this.run) this.failToStart(e);
			return;
		}
		if (run !== this.run) {
			// Cancelled while it was opening: give the microphone straight back.
			void this.deps.native.cancel();
			return;
		}
		this.phase = "listening";
		this.announce = m.crew_voice_announce_started();
		this.scheduleTick(run);
		const release = this.release;
		this.release = null;
		if (release === "latch") this.latched = true;
		else if (release === "stop") void this.stop();
	}

	private async refuse(run: number, code: string, args: Record<string, unknown>): Promise<void> {
		const text = await this.deps.coded(code, args);
		if (run !== this.run) return;
		this.phase = "error";
		this.error = { text, action: "none" };
	}

	private failToStart(e: unknown): void {
		const f = (e ?? {}) as Partial<StartFailure>;
		this.phase = "error";
		switch (f.code) {
			case "denied":
				this.error = { text: m.crew_voice_denied(), action: "settings" };
				break;
			case "no_device":
				this.error = { text: m.crew_voice_no_device(), action: "none" };
				break;
			case "unavailable":
				this.error = { text: m.crew_voice_unavailable(), action: "none" };
				break;
			case "busy":
				this.error = { text: m.crew_voice_busy(), action: "none" };
				break;
			default:
				this.error = {
					text: m.crew_voice_failed({ detail: f.detail ?? this.deps.errorText(e) }),
					action: "none",
				};
		}
	}

	/** The person stopped it (key up, second tap). */
	private async stop(): Promise<void> {
		if (this.phase !== "listening") return;
		const run = this.run;
		const take = await this.deps.native.stop();
		// null: it ended by itself a moment ago and `voice://stopped` is carrying that take.
		if (take && run === this.run && this.phase === "listening") await this.finish(take);
	}

	/** The recording is over (any reason): upload the rest and turn it into text. */
	private async finish(take: Take): Promise<void> {
		const run = this.run;
		this.cancelTick?.();
		this.cancelTick = null;
		this.phase = "transcribing";
		this.announce = this.stopAnnouncement(take.reason);
		await this.transcribeAll(run);
	}

	private stopAnnouncement(reason: Reason): string {
		switch (reason) {
			case "cap":
				return m.crew_voice_announce_cap({
					minutes: Math.round((this.deps.status()?.maxSeconds ?? 300) / 60),
				});
			case "background":
				return m.crew_voice_announce_background();
			case "device_lost":
				return m.crew_voice_announce_device_lost();
			default:
				return m.crew_voice_announce_stopped();
		}
	}

	/** Upload what was not sent yet (closing the upload) and transcribe it as a whole. */
	private async transcribeAll(run: number): Promise<void> {
		try {
			await this.inflight;
			if (run !== this.run) return;
			await this.pushAudio(true);
			const reply = parseTranscript(await this.deps.call("crewmate_transcribe", { upload: this.upload }));
			if (run !== this.run) return;
			if (reply.engineLabel) this.engine = reply.engineLabel;
			this.insert(reply.text);
		} catch (e) {
			if (run !== this.run) return;
			this.phase = "error";
			this.error = this.failure(e);
		}
	}

	/** A failed upload or transcription as the person sees it: Try again unless the same recording would be refused again. */
	private failure(e: unknown): Failure {
		const code = (e as { code?: unknown } | null)?.code;
		return {
			text: this.deps.errorText(e),
			action: typeof code === "string" && SETTLED[code] ? "none" : "retry",
		};
	}

	/** Another try with the audio the shell still holds: a new upload, the whole recording. */
	retry(): void {
		if (this.phase !== "error" || this.error?.action !== "retry") return;
		const run = ++this.run;
		this.error = null;
		this.upload = "";
		this.sent = 0;
		this.phase = "transcribing";
		this.announce = m.crew_voice_announce_stopped();
		void this.transcribeAll(run);
	}

	private insert(text: string): void {
		this.phase = "idle";
		this.partial = "";
		this.level = 0;
		const target = this.target;
		if (!target || !text.trim()) {
			this.error = { text: m.crew_voice_empty(), action: "none" };
			return;
		}
		const cur = target.read();
		const edit = insertDictation(cur.value, cur.start, cur.end, text);
		this.undo = {
			before: cur.value,
			after: edit.value,
			cursor: Math.min(cur.start, cur.end),
			dictated: this.dictated,
		};
		target.write(edit.value, edit.cursor);
		this.dictated = true;
		this.announce = m.crew_voice_announce_inserted();
	}

	// ─── the draft ─────────────────────────────────────────────────────────

	/** Escape after dictating: the draft as it was before the text came in. False when there is nothing to take back. */
	restore(): boolean {
		const u = this.undo;
		const target = this.target;
		if (!u || !target || target.read().value !== u.after) return false;
		target.write(u.before, u.cursor);
		this.dictated = u.dictated;
		this.undo = null;
		this.announce = m.crew_voice_announce_restored();
		return true;
	}

	/** The draft changed (typing, a send): the dictated text is the person's now, and an emptied draft is no dictation. */
	draftChanged(value: string): void {
		if (this.undo && value !== this.undo.after) this.undo = null;
		if (!value.trim() && !this.busy) this.dictated = false;
	}

	/** Dismiss an error. */
	clearError(): void {
		if (this.phase === "error") this.phase = "idle";
		this.error = null;
	}

	/** The error's "System settings" button. */
	openSettings(): void {
		void this.deps.native.openSettings();
	}

	// ─── the preview ───────────────────────────────────────────────────────

	private scheduleTick(run: number): void {
		this.cancelTick = this.deps.later(() => {
			this.inflight = this.refreshPreview(run).then(() => {
				if (run === this.run && this.phase === "listening") this.scheduleTick(run);
			});
		}, PARTIAL_EVERY_MS);
	}

	/** Upload the audio so far and show what the engine makes of it. A failure ends the session, audio kept. */
	private async refreshPreview(run: number): Promise<void> {
		try {
			const grew = await this.pushAudio(false);
			if (run !== this.run || this.phase !== "listening" || !grew) return;
			const reply = parseTranscript(
				await this.deps.call("crewmate_transcribe", { upload: this.upload, partial: true }),
			);
			if (run !== this.run || this.phase !== "listening") return;
			if (reply.engineLabel) this.engine = reply.engineLabel;
			this.partial = reply.text;
		} catch (e) {
			if (run !== this.run || this.phase !== "listening") return;
			// The engine refused or failed: say so now, release the microphone, keep what was said for a retry.
			await this.deps.native.stop();
			this.cancelTick?.();
			this.cancelTick = null;
			this.phase = "error";
			this.error = this.failure(e);
		}
	}

	/**
	 * Send the bytes the shell has that the server does not, in pieces. `last` closes the upload with the final
	 * piece (an empty one when everything already went). True when something was sent.
	 */
	private async pushAudio(last: boolean): Promise<boolean> {
		let any = false;
		for (;;) {
			const piece = await this.deps.native.read(this.sent, CHUNK);
			const end = this.sent + piece.data.length >= piece.total;
			const final = last && end && !piece.live;
			if (piece.data.length === 0 && !final) break;
			const reply = (await this.deps.call("crewmate_voice_put", {
				upload: this.upload,
				piece_b64: b64encode(piece.data),
				last: final,
			})) as { upload?: unknown };
			if (typeof reply?.upload === "string") this.upload = reply.upload;
			this.sent += piece.data.length;
			any ||= piece.data.length > 0;
			if (end) break;
		}
		return any;
	}

	// ─── ending ────────────────────────────────────────────────────────────

	/** End whatever is going on without a result: microphone released, audio dropped, nothing inserted. */
	private abandon(): void {
		this.run++;
		this.cancelTick?.();
		this.cancelTick = null;
		this.latched = false;
		this.release = null;
		if (this.phase === "starting" || this.phase === "listening" || this.phase === "transcribing") {
			void this.deps.native.cancel();
		}
		this.phase = "idle";
		this.partial = "";
		this.level = 0;
		this.error = null;
	}
}
