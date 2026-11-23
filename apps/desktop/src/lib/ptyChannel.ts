/**
 * Terminal output from Rust as binary frames on a Tauri channel (`crates/atlas-desktop/src/pty_feed.rs`,
 * `pty_stream.rs`): no base64, no JSON, no per-chunk script evaluation. Frame = one kind byte, then the bytes.
 * The page acknowledges what xterm has parsed (`ack`); Rust never has more than one window unacknowledged, which
 * is what bounds the webview's memory under a flood.
 */
// A namespace import: most tests mock this module with `invoke` alone, and a named import of `Channel` from such a mock
// fails at link time in every file that merely imports a terminal.
import * as tauri from "@tauri-apps/api/core";

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

/**
 * Attach `pty` (history first, then live output) and call `onFrame` for every frame, in order. Rejects when the host
 * refuses; `close` ends the stream on the Rust side.
 */
export async function openPtyStream(pty: string, onFrame: (f: PtyFrame) => void): Promise<PtyLink> {
	const channel = new tauri.Channel<ArrayBuffer>((buf) => onFrame(decodeFrame(buf)));
	const id = await tauri.invoke<number>("pty_stream", { pty, onFrame: channel });
	return {
		ack: (bytes) => void tauri.invoke("pty_stream_ack", { id, bytes }).catch(() => {}),
		close: () => {
			channel.onmessage = () => {};
			void tauri.invoke("pty_stream_stop", { id }).catch(() => {});
		},
	};
}
