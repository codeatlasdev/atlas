/**
 * The output side of one xterm.js terminal: hostd's replay of the pty's history first, then live output, in order.
 * Pure (the listener, attach and xterm calls are passed in) so the ordering is testable.
 */
export type PtyEvent = { data: string; replay: boolean };

/** The pty hostd stopped feeding because this client did not read (`pty_dropped` on `host://event`), else undefined. */
export function droppedPty(ev: unknown): string | undefined {
	const e = ev as { ev?: unknown; pty?: unknown } | null;
	return e && typeof e === "object" && e.ev === "pty_dropped" && typeof e.pty === "string"
		? e.pty
		: undefined;
}

/**
 * Bytes handed to xterm and not yet parsed, above which live output is dropped and the screen is redrawn from hostd's
 * replay instead. xterm parses in 12 ms slices (5-35 MB/s, https://xtermjs.org/docs/guides/flowcontrol/) and buffers
 * without limit until it discards data at 50 MB; "snappy for keystrokes" needs the backlog under ~500 KB there, and a
 * webview slower than that must not be fed more than it can drain. 4 MiB is a fraction of a second to parse and about
 * what hostd would replay anyway (`RING_MAX`, 2 MiB).
 */
export const BACKLOG_MAX = 4 * 1024 * 1024;

export function ptyStream(o: {
	listen: (f: (e: PtyEvent) => void) => Promise<() => void>;
	/** `pty_attach` with replay: hostd answers with one `replay` event, then live output. */
	attach: () => Promise<unknown>;
	decode: (data: string) => Uint8Array;
	/**
	 * `replay` is true for the history that follows a `reset` (the caller may reveal its view once it is drawn).
	 * `done` is called when the terminal has parsed these bytes (xterm's `write` callback); a sink without one never
	 * reports a backlog and is never throttled.
	 */
	write: (bytes: Uint8Array, replay?: boolean, done?: () => void) => void;
	reset: () => void;
}) {
	let alive = true;
	let replayed = false;
	const pending: Uint8Array[] = [];
	let off: (() => void) | undefined;
	/** Bytes written to the terminal and not yet parsed. */
	let backlog = 0;
	/** Over `BACKLOG_MAX`: live output is dropped until the terminal has caught up and the replay redraws it. */
	let behind = false;
	/** Bumped by every reset: a `done` from before it belongs to bytes the reset already forgot. */
	let epoch = 0;
	const feed = (bytes: Uint8Array, replay?: boolean) => {
		backlog += bytes.length;
		const mine = epoch;
		o.write(bytes, replay, () => {
			if (mine !== epoch) return;
			backlog -= bytes.length;
			// Caught up: now (and not before, the replay would queue behind the backlog) redraw from hostd.
			if (behind && backlog <= 0 && alive) {
				behind = false;
				replayed = false;
				attach();
			}
		});
	};
	// Attach only once the listener is registered: hostd sends the replay as soon as it hears the attach, and a
	// replay that arrives before anyone listens is gone (the terminal would hold every later byte waiting for it).
	const ready = o
		.listen((e) => {
			if (!alive) return;
			const bytes = o.decode(e.data);
			if (e.replay) {
				o.reset();
				epoch++;
				backlog = 0;
				behind = false;
				feed(bytes, true);
				replayed = true;
				pending.splice(0).forEach((b) => feed(b));
			} else if (replayed) {
				if (behind) return;
				if (backlog >= BACKLOG_MAX) behind = true;
				else feed(bytes);
			} else pending.push(bytes);
		})
		.then((u) => {
			if (alive) off = u;
			else u();
			return alive;
		});
	const attach = () => void ready.then((ok) => (ok && alive ? o.attach() : undefined)).catch(() => {});
	attach();
	return {
		/** The host came back: its replay redraws the terminal from scratch. */
		reattach() {
			replayed = false;
			attach();
		},
		dispose() {
			alive = false;
			off?.();
		},
	};
}
