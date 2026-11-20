/**
 * Voice answers to cards (#648), client side. The client only sends what was heard; hostd decides what it means
 * (`agent_approve_voice`, `crewmate_action_answer_voice`) and its verdict is rendered here. Nothing in this file (or
 * in the cards) turns words into `allow`: a transcript goes to hostd, and the only thing that can close an approval
 * is the verdict hostd reports as already decided.
 *
 * The shape is `apps/desktop/src/lib/__fixtures__/crew-voice-answers.json`, written by hostd's serializer. A verdict
 * this client does not know (a newer hostd) reads as `unclear`: nothing decided, the card stays.
 */
import { appLocale } from "#lib/i18n/locale.svelte.ts";
import { m } from "#lib/i18n/m.ts";

/** Why hostd wants a tap instead of a voice answer. */
export type TapReason = "bypass" | "rail" | "destructive" | "untrusted";

export type VoiceVerdict =
	| { verdict: "allow" }
	| { verdict: "deny" }
	/** `choice`: the option's text as the card has it. */
	| { verdict: "choice"; choice: string }
	/** `comment`: the transcript, to confirm by tap. Nothing was decided. */
	| { verdict: "comment"; comment: string }
	| { verdict: "needs_tap"; reason: TapReason | null }
	| { verdict: "unclear" };

const isObject = (v: unknown): v is Record<string, unknown> =>
	!!v && typeof v === "object" && !Array.isArray(v);

const REASONS: Record<string, TapReason> = {
	bypass: "bypass",
	rail: "rail",
	destructive: "destructive",
	untrusted: "untrusted",
};

/** The reply of `agent_approve_voice` / `crewmate_action_answer_voice`. Anything unreadable is `unclear`. */
export function parseVerdict(raw: unknown): VoiceVerdict {
	if (!isObject(raw)) return { verdict: "unclear" };
	switch (raw.verdict) {
		case "allow":
			return { verdict: "allow" };
		case "deny":
			return { verdict: "deny" };
		case "choice":
			return typeof raw.choice === "string" && raw.choice
				? { verdict: "choice", choice: raw.choice }
				: { verdict: "unclear" };
		case "comment":
			return typeof raw.comment === "string" && raw.comment.trim()
				? { verdict: "comment", comment: raw.comment }
				: { verdict: "unclear" };
		case "needs_tap":
			return {
				verdict: "needs_tap",
				reason: typeof raw.reason === "string" ? (REASONS[raw.reason] ?? null) : null,
			};
		default:
			return { verdict: "unclear" };
	}
}

/** The `lang` hostd takes with a transcript: the app's display language (`en` | `pt`). */
export const voiceLang = (): "en" | "pt" => (appLocale().startsWith("pt") ? "pt" : "en");

/** Which card is being answered: what a verdict can mean differs. */
export type CardKind = "approval" | "choice";

/** What the card does with a verdict. */
export interface VerdictView {
	/** hostd decided this: the card closes. */
	decided: { allow: boolean } | { choice: string } | null;
	/** `comment`: the text for the card's note field. */
	note: string | null;
	/** What the person reads on the card (empty when the card just closes). */
	message: string;
	/** What a screen reader says (the card's own message, or the decision). */
	announce: string;
}

const none = (message: string): VerdictView => ({ decided: null, note: null, message, announce: message });

function tapText(reason: TapReason | null): string {
	switch (reason) {
		case "bypass":
			return m.crew_voice_answer_tap_bypass();
		case "rail":
			return m.crew_voice_answer_tap_rail();
		case "destructive":
			return m.crew_voice_answer_tap_destructive();
		case "untrusted":
			return m.crew_voice_answer_tap_untrusted();
		default:
			return m.crew_voice_answer_tap_other();
	}
}

/**
 * A verdict as the card of `kind` shows it. `heard` is what the person said, echoed in the messages that need it.
 * A verdict that cannot belong to this card (an approval answered with a `choice`) is shown as `unclear` and
 * closes nothing: the refresh that follows every voice call shows what hostd really did.
 */
export function verdictView(v: VoiceVerdict, kind: CardKind, heard: string): VerdictView {
	const unclear = () =>
		none(
			heard.trim()
				? m.crew_voice_answer_unclear({ text: heard.trim() })
				: m.crew_voice_answer_unclear_empty(),
		);
	switch (v.verdict) {
		case "allow":
			return kind === "approval"
				? { decided: { allow: true }, note: null, message: "", announce: m.crew_voice_answer_allowed() }
				: unclear();
		case "deny":
			return kind === "approval"
				? { decided: { allow: false }, note: null, message: "", announce: m.crew_voice_answer_denied() }
				: unclear();
		case "choice":
			return kind === "choice"
				? {
						decided: { choice: v.choice },
						note: null,
						message: "",
						announce: m.crew_voice_answer_picked({ choice: v.choice }),
					}
				: unclear();
		case "comment": {
			if (kind !== "approval") return unclear();
			const message = m.crew_voice_answer_comment({ text: v.comment.trim() });
			return { decided: null, note: v.comment.trim(), message, announce: message };
		}
		case "needs_tap":
			return none(tapText(v.reason));
		default:
			return unclear();
	}
}
