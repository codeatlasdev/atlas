import { createHash } from "node:crypto";

// The Atlas earcon set as data: what each sound means, the ElevenLabs prompt that asks for it, and the measurable
// spec a candidate must meet before it can ship. Pure (no I/O) so `spec.test.ts` can pin the rules.
//
// Palette: ONE timbre family for every earcon (a warm glass-marimba mallet, dry) so ten sounds read as one brand,
// like the Mac chime is one voice. Meaning rides on pitch contour and count of notes, not on a different instrument:
// rising = progress/good, falling = problem, two notes = "look at me", one note = acknowledge.

/** Timbre sentence appended to every prompt. Changing it re-voices the whole set; do it deliberately. */
export const FAMILY = "warm glassy marimba mallet tone, soft attack, clean and dry";

export interface Earcon {
	/** `signature` is the one ownable sound (app open); everything else is feedback. */
	kind: "signature" | "feedback";
	/** What happens in the product. */
	meaning: string;
	/** The musical gesture, without the timbre. */
	gesture: string;
	/** Seconds asked from the API (its minimum is 0.5); the file is trimmed to the sound afterwards. */
	requestSeconds: number;
	/** Longest the finished file may be. */
	maxSeconds: number;
	/** Brightness the picker prefers (spectral centroid, Hz): urgent sounds are brighter, ambient ones rounder. */
	targetCentroidHz: number;
	/** Signatures only: the shortest finished file that still counts as a sonic logo. */
	minSeconds?: number;
	/** Signatures only: seconds into the finished file where its downbeat (the strike the picture is cut to) falls. */
	downbeatSeconds?: number;
}

export const EARCONS = {
	boot: {
		kind: "signature",
		meaning: "Atlas opened (cold start, at most once an hour)",
		gesture: "three note ascending arpeggio resolving upward, calm and confident, a gentle bloom on the last note",
		requestSeconds: 1.5,
		maxSeconds: 0.9,
		targetCentroidHz: 700,
	},
	"discovery-done": {
		kind: "feedback",
		meaning: "Find my projects finished",
		gesture: "two quick soft rising notes, a fifth apart, gentle level, tidy and satisfying like a completed scan",
		requestSeconds: 0.8,
		maxSeconds: 0.3,
		targetCentroidHz: 900,
	},
	"stack-up": {
		kind: "feedback",
		meaning: "Every service of a stack is healthy",
		gesture: "two notes rising a fourth then settling, warm and resolved, a small celebration",
		requestSeconds: 0.8,
		maxSeconds: 0.3,
		targetCentroidHz: 800,
	},
	"service-failed": {
		kind: "feedback",
		meaning: "A service crashed",
		gesture: "two notes falling a minor third, slightly firmer attack, serious but not alarming",
		requestSeconds: 0.8,
		maxSeconds: 0.3,
		targetCentroidHz: 650,
	},
	"agent-needs-you": {
		kind: "feedback",
		meaning: "An agent is waiting for you",
		gesture: "two identical notes then a higher third, a polite tap-tap-ask, inviting",
		requestSeconds: 0.8,
		maxSeconds: 0.3,
		targetCentroidHz: 1000,
	},
	"agent-done": {
		kind: "feedback",
		meaning: "An agent finished",
		gesture: "single soft note with a short bright overtone, a quiet checkmark",
		requestSeconds: 0.6,
		maxSeconds: 0.3,
		targetCentroidHz: 800,
	},
	update: {
		kind: "feedback",
		meaning: "An update is ready",
		gesture: "two notes rising a major second, light and optimistic",
		requestSeconds: 0.8,
		maxSeconds: 0.3,
		targetCentroidHz: 850,
	},
	error: {
		kind: "feedback",
		meaning: "An action failed (soft)",
		gesture: "one low muted note with a tiny downward slide, a soft damped thud that dies away quickly, not a buzzer",
		requestSeconds: 0.6,
		maxSeconds: 0.3,
		targetCentroidHz: 500,
	},
	success: {
		kind: "feedback",
		meaning: "Copied or saved",
		gesture: "single very short high tick, tiny and crisp",
		requestSeconds: 0.5,
		maxSeconds: 0.25,
		targetCentroidHz: 1300,
	},
	toggle: {
		kind: "feedback",
		meaning: "A meaningful switch changed",
		gesture: "one extremely short damped muted tick, a tiny soft wooden tap with almost no ring, dying away within a fifth of a second",
		requestSeconds: 0.5,
		maxSeconds: 0.2,
		targetCentroidHz: 900,
	},
} as const satisfies Record<string, Earcon>;

export type EarconId = keyof typeof EARCONS;

/** The earcons as plain `Earcon`s, for code that reads optional fields (`as const` keeps only the ones each entry has). */
export const EARCON_LIST: Record<EarconId, Earcon> = EARCONS;

/** The text-to-sound prompt for one earcon. */
export function promptFor(e: Earcon): string {
	return `${e.gesture}. ${FAMILY}, no reverb tail, no background noise, no voice, user interface sound effect.`;
}

/** Digest of everything that changes what the model returns for `id`: its prompt and the requested length. */
function promptKey(id: EarconId): string {
	const e = EARCONS[id];
	return createHash("sha256").update(JSON.stringify({ id, prompt: promptFor(e), seconds: e.requestSeconds, model: MODEL })).digest("hex").slice(0, 16);
}

/** Model the files come from; part of the key so a model change retires every old take. */
export const MODEL = "eleven_text_to_sound_v2";

/** Cache file name of take `variant` of `id`; the key ties it to the prompt that made it. */
export const takeName = (id: EarconId, variant: number) => `${id}.${variant}.${promptKey(id)}.mp3`;

/** Whether `file` was generated from `id`'s current prompt (older takes stay on disk but are never picked). */
export const isCurrentTake = (id: EarconId, file: string) => {
	const m = /^(.+)\.(\d+)\.([0-9a-f]{16})\.mp3$/.exec(file);
	return !!m && m[1] === id && m[3] === promptKey(id);
};

/** What `ffmpeg` measured on a trimmed candidate. */
export interface Stats {
	/** Seconds of sound after trimming leading/trailing silence. */
	duration: number;
	peakDb: number;
	rmsDb: number;
	/** Spectral centroid in Hz (brightness). */
	centroidHz: number;
	clipped: boolean;
	/** Seconds of silence before the attack in the raw take. */
	silentHead: number;
	/** Level (dB re the take's peak) of the last 5 ms before the earcon's cap; only measured for takes longer than it. */
	levelAtCapDb?: number;
}

export type Verdict = { ok: true } | { ok: false; why: "clipping" | "too-long" | "cut-off" | "too-short" | "too-bright" | "too-dull" | "late-attack" };

/**
 * Shortest sound that is still heard as a note, the brightness window of our timbre family, and the attack lag allowed.
 * The window comes from measuring the 40 real takes of the first generation: a mallet tone is fundamental-heavy, so its
 * energy-weighted spectral centroid sits at 400-1600 Hz, not at the 2-3 kHz a bell or a click would have.
 */
const MIN_SECONDS = 0.06;
const CENTROID_FLOOR = 350;
const CENTROID_CEILING = 3200;
const MAX_HEAD = 0.05;
/** A note is over when it has decayed this far below its own peak. */
export const DECAY_DB = 30;
/** A signature's bell rings on quietly and that tail is part of its character: it is cut deeper. */
const SIGNATURE_DECAY_DB = 40;
export const decayDb = (e: Earcon) => (e.kind === "signature" ? SIGNATURE_DECAY_DB : DECAY_DB);
/** A take longer than its cap is faded out at the cap, but only if it is at least this far below its peak there. */
export const CAP_QUIET_DB = -18;
/** Fade-out applied at the cap: 30 ms, or a quarter of the cap for tiny earcons so the fade never eats the sound. */
export const capFade = (capSeconds: number) => Math.min(0.03, capSeconds / 4);
/** Takes longer than this multiple of the cap are the model padding, not a note that rings a little long. */
const MAX_OVERSHOOT = 2.5;

/** Hard spec: a candidate that fails any of these is never picked, however it scores otherwise. */
export function passes(e: Earcon, s: Stats): Verdict {
	if (s.clipped) return { ok: false, why: "clipping" };
	if (s.duration > e.maxSeconds) {
		if (s.levelAtCapDb === undefined || s.duration > e.maxSeconds * MAX_OVERSHOOT) return { ok: false, why: "too-long" };
		if (s.levelAtCapDb > CAP_QUIET_DB) return { ok: false, why: "cut-off" };
	}
	if (s.duration < Math.max(MIN_SECONDS, e.minSeconds ?? 0)) return { ok: false, why: "too-short" };
	if (s.centroidHz > CENTROID_CEILING) return { ok: false, why: "too-bright" };
	if (s.centroidHz < CENTROID_FLOOR) return { ok: false, why: "too-dull" };
	if (e.kind !== "signature" && s.silentHead > MAX_HEAD) return { ok: false, why: "late-attack" };
	return { ok: true };
}

/** Among the candidates that pass, the one closest to the target brightness; ties break by id. */
export function pickBest<T extends { id: string; stats: Stats }>(e: Earcon, candidates: T[]): T | undefined {
	return candidates
		.filter((c) => passes(e, c.stats).ok)
		.sort(
			(a, b) =>
				Math.abs(a.stats.centroidHz - e.targetCentroidHz) - Math.abs(b.stats.centroidHz - e.targetCentroidHz) ||
				a.id.localeCompare(b.id),
		)[0];
}
