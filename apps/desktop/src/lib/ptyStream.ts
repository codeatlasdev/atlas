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
}) {
	let alive = true;
	let link: PtyLink | undefined;
	/**
	 * Parsed bytes not yet reported: frames can arrive before the attach's answer (the stream's id) does, and an ack
	 * lost there would leave that much of the window used for ever.
	 */
	let owed = 0;
	/** Bumped by every attach: frames and parse callbacks of an earlier stream are stale. */
	let epoch = 0;
	const attach = () => {
		const mine = ++epoch;
		link?.close();
		link = undefined;
		owed = 0;
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
				case "dropped":
					attach();
					break;
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
	const feed = (bytes: Uint8Array, replay: boolean, mine: number) => {
		o.write(bytes, replay, () => {
			if (mine !== epoch) return;
			if (link) link.ack(bytes.length);
			else owed += bytes.length;
		});
	};
	attach();
	return {
		/** The host came back: its replay redraws the terminal from scratch. */
		reattach: attach,
		dispose() {
			alive = false;
			epoch++;
			link?.close();
		},
	};
}
