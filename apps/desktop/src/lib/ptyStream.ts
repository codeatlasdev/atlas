/**
 * The output side of one xterm.js terminal: hostd's replay of the pty's history first, then live output, in order.
 * Pure (the listener, attach and xterm calls are passed in) so the ordering is testable.
 */
export type PtyEvent = { data: string; replay: boolean };

/** The pty hostd stopped feeding because this client did not read (`pty_dropped` on `host://event`), else undefined. */
export function droppedPty(ev: unknown): string | undefined {
	const e = ev as { ev?: unknown; pty?: unknown } | null;
	return e && typeof e === "object" && e.ev === "pty_dropped" && typeof e.pty === "string" ? e.pty : undefined;
}

export function ptyStream(o: {
	listen: (f: (e: PtyEvent) => void) => Promise<() => void>;
	/** `pty_attach` with replay: hostd answers with one `replay` event, then live output. */
	attach: () => Promise<unknown>;
	decode: (data: string) => Uint8Array;
	/** `replay` is true for the history that follows a `reset` (the caller may reveal its view once it is drawn). */
	write: (bytes: Uint8Array, replay?: boolean) => void;
	reset: () => void;
}) {
	let alive = true;
	let replayed = false;
	const pending: Uint8Array[] = [];
	let off: (() => void) | undefined;
	// Attach only once the listener is registered: hostd sends the replay as soon as it hears the attach, and a
	// replay that arrives before anyone listens is gone (the terminal would hold every later byte waiting for it).
	const ready = o
		.listen((e) => {
			if (!alive) return;
			const bytes = o.decode(e.data);
			if (e.replay) {
				o.reset();
				o.write(bytes, true);
				replayed = true;
				pending.splice(0).forEach((b) => o.write(b));
			} else if (replayed) o.write(bytes);
			else pending.push(bytes);
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
