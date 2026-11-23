/**
 * Terminal output from Rust as binary frames on a Tauri channel (`crates/atlas-desktop/src/pty_feed.rs`,
 * `pty_stream.rs`): no base64, no JSON, no per-chunk script evaluation. Frame = one kind byte, then the bytes.
 * The page acknowledges what xterm has parsed (`ack`); Rust never has more than one window unacknowledged, which
 * is what bounds the webview's memory under a flood.
 */
import { invoke } from "@tauri-apps/api/core";

/** Kind bytes, as `pty_feed.rs` writes them. */
const DATA = 0;
const REPLAY = 1;
const DROPPED = 2;
const ENDED = 3;

/** `replay`: reset the view, then draw these bytes (hostd's history; empty when there is none). */
export type PtyFrame = { kind: "data" | "replay"; bytes: Uint8Array } | { kind: "dropped" | "ended" };

/** One frame as the channel delivers it (an `ArrayBuffer`); the bytes are a view of it, not a copy. */
export function decodeFrame(buf: ArrayBuffer): PtyFrame {
	const b = new Uint8Array(buf);
	switch (b[0]) {
		case DATA:
			return { kind: "data", bytes: b.subarray(1) };
		case REPLAY:
			return { kind: "replay", bytes: b.subarray(1) };
		case DROPPED:
			return { kind: "dropped" };
		case ENDED:
			return { kind: "ended" };
		default:
			throw new Error(`unknown terminal frame kind ${b[0]}`);
	}
}

/** What a view holds of one open stream. */
export type PtyLink = {
	/** The terminal parsed `bytes` of what was sent. */
	ack: (bytes: number) => void;
	close: () => void;
};

/** The part of Tauri's page-side runtime a channel needs (`@tauri-apps/api/core` calls the same two functions). */
type Internals = {
	transformCallback: (cb: (raw: never) => void) => number;
	unregisterCallback: (id: number) => void;
};

/**
 * What `new Channel(cb)` of `@tauri-apps/api/core` does for messages that arrive in order or not (the fetch path of
 * a frame of 1 KiB or more is asynchronous, so a later small frame can overtake it): `cb` gets them in the order Rust
 * sent them. Written here and not imported because the library's `Channel` (with the class helpers it needs) lands
 * in the chunk that `invoke` lives in, which every start of the app loads: 1 KiB raw / 0.4 KiB gzip of startup JS
 * that only a terminal uses (`scripts/bundle-budget.ts`). `id` goes to Rust as `__CHANNEL__:<id>`.
 */
export function orderedChannel<T>(
	internals: Internals,
	onmessage: (message: T) => void,
): { id: number; mute: () => void; release: () => void } {
	let next = 0;
	let endAt: number | undefined;
	let muted = false;
	const pending = new Map<number, T>();
	// Released when Rust's end of the channel is dropped and everything before its end message was delivered.
	const release = () => internals.unregisterCallback(id);
	const deliver = (message: T) => {
		if (muted) return;
		try {
			onmessage(message);
		} catch (e) {
			// A throwing handler must not stall the channel (the index would never advance): report it, go on.
			queueMicrotask(() => {
				throw e;
			});
		}
	};
	const id = internals.transformCallback(((raw: { index: number; message?: T; end?: true }) => {
		if (raw.end) {
			if (raw.index === next) release();
			else endAt = raw.index;
			return;
		}
		if (raw.index !== next) {
			pending.set(raw.index, raw.message as T);
			return;
		}
		deliver(raw.message as T);
		next++;
		while (pending.has(next)) {
			const m = pending.get(next) as T;
			pending.delete(next);
			deliver(m);
			next++;
		}
		if (next === endAt) release();
	}) as never);
	return {
		id,
		// Frames still on their way are dropped; the callback stays registered until Rust's end message (a closed
		// stream's last frames would otherwise find no callback).
		mute: () => {
			muted = true;
			pending.clear();
		},
		release,
	};
}

/**
 * Attach `pty` (history first, then live output) and call `onFrame` for every frame, in order. Rejects when the host
 * refuses; `close` ends the stream on the Rust side.
 */
export async function openPtyStream(pty: string, onFrame: (f: PtyFrame) => void): Promise<PtyLink> {
	const channel = orderedChannel<ArrayBuffer>(
		(window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__,
		(buf) => onFrame(decodeFrame(buf)),
	);
	let id: number;
	try {
		id = await invoke<number>("pty_stream", { pty, onFrame: `__CHANNEL__:${channel.id}` });
	} catch (e) {
		channel.mute();
		channel.release();
		throw e;
	}
	return {
		ack: (bytes) => void invoke("pty_stream_ack", { id, bytes }).catch(() => {}),
		close: () => {
			channel.mute();
			void invoke("pty_stream_stop", { id }).catch(() => {});
		},
	};
}
