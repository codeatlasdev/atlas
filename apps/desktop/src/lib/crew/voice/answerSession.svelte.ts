/**
 * Answering a card by voice (#648): the card's microphone. It records and transcribes exactly like the message box
 * (a `Dictation`, same recorder, same upload, same `crewmate_transcribe`), but the transcript does not go into a
 * draft: it goes to hostd (`submit`), which decides what it means and replies with a verdict. This class never maps
 * words to an answer itself; it only hands the verdict to the card (`onview`) and keeps the message to show.
 */
import { errText } from "#lib/api.ts";
import { Dictation, type Deps, type Target } from "./dictation.svelte.ts";
import { parseVerdict, verdictView, type CardKind, type VerdictView, type VoiceVerdict } from "./answers.ts";

export class VoiceAnswer {
	readonly dictation: Dictation;
	/** What the last answer said, for the card to show under the buttons. */
	message = $state("");
	/** What a screen reader hears about the verdict (the decision, or the same sentence the card shows). */
	announce = $state("");
	/** The transcript is with hostd. */
	sending = $state(false);

	/** The dictation's text arrives as a write into this "draft": the card has no draft to fill. */
	private readonly target: Target = {
		read: () => ({ value: "", start: 0, end: 0 }),
		write: (value) => void this.heard(value),
	};

	constructor(
		deps: Deps,
		private readonly kind: CardKind,
		/** Sends the transcript to hostd (`agent_approve_voice` / `crewmate_action_answer_voice`); resolves its raw reply. */
		private readonly submit: (transcript: string) => Promise<unknown>,
		/** The card learns what hostd said (close, fill the note); called for every verdict. */
		private readonly onview: (v: VoiceVerdict, view: VerdictView) => void,
	) {
		this.dictation = new Dictation(deps);
	}

	get busy(): boolean {
		return this.dictation.busy || this.sending;
	}

	attach(): Promise<void> {
		return this.dictation.attach();
	}

	dispose(): void {
		this.dictation.dispose();
	}

	/** The card's microphone button (click: starts and stays on; the next click stops and sends). */
	toggle(): void {
		if (this.sending) return;
		this.message = "";
		this.announce = "";
		this.dictation.toggle(this.target);
	}

	/** Esc: stop and forget what was said. */
	cancel(): void {
		this.dictation.cancel();
	}

	private async heard(text: string): Promise<void> {
		this.sending = true;
		try {
			const v = parseVerdict(await this.submit(text));
			const view = verdictView(v, this.kind, text);
			this.message = view.message;
			this.announce = view.announce;
			this.onview(v, view);
		} catch (e) {
			// A refusal of hostd (no `approve` scope, the request already answered, ...) is said as it is.
			this.message = errText(e);
		} finally {
			this.sending = false;
		}
	}
}
