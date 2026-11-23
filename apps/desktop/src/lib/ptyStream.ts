/**
 * The output side of one xterm.js terminal: hostd's history first (a reset), then live output, in order, fed from a
 * binary channel (`ptyChannel.ts`). Pure (the stream and xterm calls are passed in) so the ordering is testable.
 *
 * Memory is bounded on the Rust side: it sends at most a window of bytes that this module has not reported parsed
 * (`link.ack`, from xterm's `write` callback), so a program that prints faster than xterm parses waits in hostd's
 * queue (and then in the program) instead of piling up in the webview.
 */
import type { PtyFrame, PtyLink } from "#lib/ptyChannel.ts";

/** The pty hostd stopped feeding because this client did not read (`pty_dropped` on `host://event`), else undefined. */
export function droppedPty(ev: unknown): string | undefined {
	const e = ev as { ev?: unknown; pty?: unknown } | null;
	return e && typeof e === "object" && e.ev === "pty_dropped" && typeof e.pty === "string"
		? e.pty
		: undefined;
}

/** A stream that ends `dropped` within this long of opening is a failed attach, not a reader that stalled. */
const QUICK_MS = 2000;
const BACKOFF_MS = 250;
const MAX_BACKOFF_MS = 30_000;
/**
 * Parsed bytes are reported in batches: one acknowledgement is an IPC call, and a chat that streams ~30 updates a
 * second of ~500 bytes would make 30 calls a second to credit 15 KB. A batch goes out when it reaches `ACK_BYTES` (a
 * flood parses megabytes at a time, so it is never delayed) or `ACK_MS` after its first byte, so Rust's window
 * (2 MiB) is never short of more than `ACK_BYTES` of parsed output for longer than that.
 */
export const ACK_BYTES = 64 * 1024;
export const ACK_MS = 500;

export function ptyStream(o: {
	/** Attach with replay: frames start with the history. */
	open: (onFrame: (f: PtyFrame) => void) => Promise<PtyLink>;
	/** After every (re)attach: fit the terminal and tell the pty its size. */
	attached: () => Promise<unknown>;
	/**
	 * `replay` is true for the history that follows a `reset` (the caller may reveal its view once it is drawn).
	 * `done` is called when the terminal has parsed these bytes (xterm's `write` callback).
	 */
	write: (bytes: Uint8Array, replay: boolean, done: () => void) => void;
	reset: () => void;
	/** Resolves after `ms`; injected so the tests drive the clock. */
	wait?: (ms: number) => Promise<void>;
}) {
	const wait = o.wait ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
	let alive = true;
	let link: PtyLink | undefined;
	/**
	 * Parsed bytes not yet reported: frames can arrive before the attach's answer (the stream's id) does, and an ack
	 * lost there would leave that much of the window used for ever.
	 */
	let owed = 0;
	/** Parsed bytes of the current stream not reported yet, and whether a timer will report them. */
	let unacked = 0;
	let ackTimer = false;
	/**
	 * Bytes handed to xterm and not parsed yet, over every stream this view had: `reset()` does not drop the writes xterm
	 * queued, so a new stream (a fresh window on Rust's side) opened on top of a backlog adds to it, and a slow page
	 * would pile up streams until xterm discards data. A redraw waits for it to reach zero.
	 */
	let backlog = 0;
	/** A redraw was asked for while xterm still held output. */
	let wanted = false;
	/** Bumped by every attach: frames and parse callbacks of an earlier stream are stale. */
	let epoch = 0;
	/**
	 * Streams that ended `dropped` soon after they opened, one after the other. Redrawing from the history is right
	 * when hostd cut a reader that stalled; when the host closes every stream right away (a pty that just ended, a
	 * host that refuses) it is a loop of attach, replay of the whole history and re-parse, ~10 times a second, for
	 * as long as it lasts. The first quick drop redraws at once; each one after it waits twice as long as the one before.
	 */
	let quick = 0;
	let openedAt = 0;
	/** Redraw from hostd's history: now when xterm is caught up, else as soon as it is (the old stream stops at once). */
	const attach = () => {
		epoch++;
		link?.close();
		link = undefined;
		owed = 0;
		unacked = 0;
		if (backlog > 0) wanted = true;
		else open();
	};
	const open = () => {
		wanted = false;
		const mine = epoch;
		openedAt = Date.now();
		o.open((f) => {
			if (!alive || mine !== epoch) return;
			switch (f.kind) {
				case "replay":
					o.reset();
					feed(f.bytes, true, mine);
					break;
				case "data":
					feed(f.bytes, false, mine);
					break;
				// hostd cut this reader: redraw from its history.
				case "dropped": {
					quick = Date.now() - openedAt < QUICK_MS ? quick + 1 : 0;
					// The first drop redraws at once (a reader that stalled); only a second one in a row waits.
					if (quick <= 1) {
						attach();
						break;
					}
					link?.close();
					link = undefined;
					void wait(Math.min(MAX_BACKOFF_MS, BACKOFF_MS * 2 ** (quick - 2))).then(() => {
						if (alive && mine === epoch) attach();
					});
					break;
				}
				case "ended":
					break;
			}
		}).then(
			(l) => {
				if (!alive || mine !== epoch) l.close();
				else {
					link = l;
					if (owed) l.ack(owed);
					owed = 0;
					void o.attached().catch(() => {});
				}
			},
			() => {},
		);
	};
	const flush = () => {
		if (link && unacked > 0) link.ack(unacked);
		unacked = 0;
	};
	const feed = (bytes: Uint8Array, replay: boolean, mine: number) => {
		backlog += bytes.length;
		o.write(bytes, replay, () => {
			backlog -= bytes.length;
			if (wanted && backlog <= 0 && alive) return open();
			if (mine !== epoch) return;
			if (!link) owed += bytes.length;
			else if ((unacked += bytes.length) >= ACK_BYTES) flush();
			else if (!ackTimer) {
				ackTimer = true;
				void wait(ACK_MS).then(() => {
					ackTimer = false;
					if (alive) flush();
				});
			}
		});
	};
	attach();
	return {
		/** The host came back: its replay redraws the terminal from scratch. */
		reattach: () => {
			quick = 0;
			attach();
		},
		dispose() {
			alive = false;
			epoch++;
			link?.close();
		},
	};
}
