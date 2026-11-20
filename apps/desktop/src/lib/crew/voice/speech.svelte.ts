/**
 * Reading replies aloud (#648): the state of the one reply being spoken by the shell's synthesizer
 * (`crates/atlas-desktop/src/speech`). The shell speaks with the system's voice and rate; this class decides what is
 * said (what hostd computed as `task.spoken`, nothing when it is empty), in which language (the app's), and when to
 * stop: at once on any key or click, when dictation starts, when the chat goes away. Never the Web Speech API.
 *
 * Everything outside (the shell, the window, the catalog) is injected, so the rules are tested with fakes.
 */
import { m } from "#lib/i18n/m.ts";

/** The shell's side (Tauri commands and the end event). */
export interface SpeechNative {
	/** Whether this system can speak (Linux without speech-dispatcher cannot). */
	available(): Promise<boolean>;
	/** Speak, cutting what is being said; resolves with the utterance id. Rejects with a reason. */
	speak(text: string, lang: string): Promise<number>;
	stop(): Promise<void>;
	/** An utterance ran out by itself (never after a stop). */
	onEnded(cb: (id: number) => void): Promise<() => void>;
}

/** What the window offers for "any key or click". */
export type Interruptions = Pick<EventTarget, "addEventListener" | "removeEventListener">;

export interface SpeechDeps {
	native: SpeechNative;
	/** The language to speak in: the app's display language (`en`, `pt-BR`). */
	lang(): string;
	errorText(e: unknown): string;
}

/** The events that silence a reply: a key, a pointer, a click. */
const INTERRUPTS = ["keydown", "pointerdown", "mousedown"] as const;

export class Speech {
	/** A reply is being spoken now. */
	speaking = $state(false);
	/** `null` until the shell has answered. */
	available = $state<boolean | null>(null);
	/** What a screen reader hears. */
	announce = $state("");
	/** Why the last utterance could not be spoken. */
	error = $state<string | null>(null);

	/** Bumped by every start and stop, so a late answer of the shell knows it is stale. */
	private run = 0;
	private current = 0;
	private off: (() => void)[] = [];
	private mounts = 0;

	constructor(private readonly deps: SpeechDeps) {}

	/** Listen to the shell and to the window. Call once when the chat mounts; `dispose` undoes it. */
	async attach(win: Interruptions): Promise<void> {
		const mine = ++this.mounts;
		const stopOnInput = () => {
			if (this.speaking) this.stop();
		};
		for (const ev of INTERRUPTS) win.addEventListener(ev, stopOnInput, true);
		const unlisten = await this.deps.native.onEnded((id) => {
			if (id !== this.current || !this.speaking) return;
			this.speaking = false;
			this.current = 0;
		});
		const release = () => {
			for (const ev of INTERRUPTS) win.removeEventListener(ev, stopOnInput, true);
			unlisten();
		};
		if (mine !== this.mounts) {
			release();
			return;
		}
		this.off.push(release);
		this.available = await this.deps.native.available().catch(() => false);
	}

	/** The chat is going away: silence, stop listening. */
	dispose(): void {
		this.mounts++;
		this.off.forEach((f) => f());
		this.off = [];
		this.stop();
	}

	/** Say `text` (what hostd computed for listening). Blank: nothing, and nothing already being said is cut. */
	async speak(text: string): Promise<void> {
		const said = text.trim();
		if (!said || this.available === false) return;
		const run = ++this.run;
		this.error = null;
		try {
			const id = await this.deps.native.speak(said, this.deps.lang());
			if (run !== this.run) return;
			this.current = id;
			this.speaking = true;
			this.announce = m.crew_voice_announce_speaking();
		} catch (e) {
			if (run !== this.run) return;
			this.speaking = false;
			this.error = m.crew_voice_speak_failed({ detail: this.deps.errorText(e) });
		}
	}

	/** Silence at once (also a no-op when nothing is being said, and it cancels one still starting). */
	stop(): void {
		const was = this.speaking;
		this.run++;
		this.current = 0;
		this.speaking = false;
		void this.deps.native.stop().catch(() => {});
		if (was) this.announce = m.crew_voice_announce_speaking_stopped();
	}
}
