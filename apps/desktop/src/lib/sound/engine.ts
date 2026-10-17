/**
 * Web Audio playback of the earcons. One AudioContext per session, created on the first user gesture
 * (WebKit and Chrome both refuse to start audio before one), assets fetched and decoded once.
 * Everything environmental (context class, asset loader, prefs/clock/focus) is injected so the control flow
 * is testable without a browser audio stack; `sound.svelte.ts` wires the real ones.
 */
import { PERSISTED, decide, gain, record, seedMemory, type Category, type Ctx } from "./policy.ts";

/** The slice of `AudioContext` the engine uses. */
interface AudioCtxLike {
	state: string;
	destination: unknown;
	resume(): unknown;
	decodeAudioData(data: ArrayBuffer): Promise<unknown>;
	createBufferSource(): {
		buffer: unknown;
		connect(to: unknown): unknown;
		start(when?: number): void;
	};
	createGain(): { gain: { value: number }; connect(to: unknown): unknown };
}

export interface EngineDeps {
	AudioContext: (new () => AudioCtxLike) | undefined;
	/** Raw bytes of an earcon (`/sounds/<name>.m4a` in the app). */
	fetchAsset: (name: Category) => Promise<ArrayBuffer>;
	/** Live world state, read at call time. `now` is the clock. */
	context: () => Ctx;
	/** Last-played times that must survive a restart (`{ boot: ms }`), and where to write them back. */
	persisted?: { load: () => Record<string, unknown>; save: (last: Record<string, number>) => void };
}

export interface Engine {
	/** Call from any pointer/key event: creates the context once, resumes it while suspended. */
	gesture(): void;
	/** Play `category` if the policy allows. Never throws; resolves after the sound was started (or dropped). */
	play(category: Category, opts?: { preview?: boolean }): Promise<void>;
	/**
	 * Like `play`, but a request made before any gesture is held (the latest one wins) and played once, through the
	 * policy, when the first gesture creates the context. For the cold-start signature, which is asked for at launch.
	 */
	playOnGesture(category: Category): void;
}

export function createEngine(deps: EngineDeps): Engine {
	const memory = seedMemory(deps.persisted?.load() ?? {});
	const buffers = new Map<Category, Promise<unknown>>();
	let ctx: AudioCtxLike | null = null;
	let held: Category | null = null;
	let deviceFailed = false;

	function decoded(category: Category, c: AudioCtxLike) {
		let p = buffers.get(category);
		if (!p) {
			p = deps.fetchAsset(category).then((bytes) => c.decodeAudioData(bytes));
			buffers.set(category, p);
			// A failed load is retried next time instead of being cached as a permanent miss.
			p.catch(() => buffers.delete(category));
		}
		return p;
	}

	async function play(category: Category, opts: { preview?: boolean } = {}) {
		// No context yet means no gesture yet: drop the sound, never queue it for a surprise later.
		const c = ctx;
		if (!c) return;
		const world = deps.context();
		if (!decide(category, world, memory, opts).play) return;
		try {
			const buffer = await decoded(category, c);
			const src = c.createBufferSource();
			const g = c.createGain();
			src.buffer = buffer;
			g.gain.value = gain(category, world.prefs);
			src.connect(g);
			g.connect(c.destination);
			src.start();
			if (!opts.preview) {
				record(category, world.now, memory);
				if (PERSISTED.includes(category)) deps.persisted?.save({ [category]: world.now });
			}
		} catch {
			// A missing or undecodable asset is a silent miss: sound is never the only channel.
		}
	}

	return {
		play,

		gesture() {
			if (!deps.AudioContext) return;
			ctx ??= new deps.AudioContext();
			// #87: WebKitGTK without a usable audio device rejects resume(); unhandled, every gesture surfaced as an
			// app error. A device that failed to start stays silent for the session instead of being retried.
			if (ctx.state === "suspended" && !deviceFailed)
				Promise.resolve(ctx.resume()).catch(() => {
					deviceFailed = true;
				});
			if (held) {
				const category = held;
				held = null;
				void play(category);
			}
		},

		playOnGesture(category) {
			if (ctx) void play(category);
			else held = category;
		},
	};
}
