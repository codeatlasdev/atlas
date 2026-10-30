/**
 * One listener per hostd event name for the whole page, routed to the terminals (and chat tabs) it concerns.
 *
 * Every mounted terminal used to register its own `listen("host://event")` and `listen("host://reconnected")`: two IPC
 * round trips each, and a JS callback per terminal for EVERY hostd event (`pty_activity`, `pty_title`, `fs_event`,
 * `lsp_*`). On WebKitGTK, where the main thread is the scarce resource, that made the cost grow with the number of
 * tabs. Here the listeners are created on the first subscriber and released with the last one.
 *
 * Pure (Tauri's `listen` is passed in), so the routing is testable.
 */
import { droppedPty } from "#lib/ptyStream.ts";

type Listen = (name: string, cb: (e: { payload: unknown }) => void) => Promise<() => void>;

/** A lazily created, reference-counted `listen` for one event name. */
interface SharedListener {
	acquire(): void;
	release(): void;
}

function shared(listen: Listen, name: string, onEvent: (payload: unknown) => void): SharedListener {
	let users = 0;
	let off: Promise<() => void> | undefined;
	return {
		acquire() {
			if (users++ === 0) off = listen(name, (e) => onEvent(e.payload));
		},
		release() {
			if (--users > 0) return;
			// The answer may still be on its way (a fast mount/unmount): release whatever it brings.
			const pending = off;
			off = undefined;
			void pending?.then((u) => u());
		},
	};
}

export function createHostEvents(listen: Listen) {
	const dropped = new Map<string, Set<() => void>>();
	const reconnected = new Set<() => void>();
	const run = (fns: Iterable<() => void>) => {
		for (const f of [...fns]) {
			try {
				f();
			} catch (e) {
				console.error("host event subscriber failed", e);
			}
		}
	};
	const events = shared(listen, "host://event", (p) => {
		const pty = droppedPty(p);
		if (pty !== undefined) run(dropped.get(pty) ?? []);
	});
	const reconnects = shared(listen, "host://reconnected", () => run(reconnected));

	/** Add `fn` to `set`, hold the shared listener while it is there; the returned off is idempotent. */
	function join(set: Set<() => void>, fn: () => void, src: SharedListener, after?: () => void) {
		// A fresh wrapper per subscription: the same `fn` subscribed twice is two subscriptions.
		const entry = () => fn();
		set.add(entry);
		src.acquire();
		return () => {
			if (!set.delete(entry)) return;
			after?.();
			src.release();
		};
	}

	return {
		/** hostd stopped feeding `pty` because this page did not read (`pty_dropped`). */
		onDropped(pty: string, fn: () => void) {
			const set = dropped.get(pty) ?? dropped.set(pty, new Set()).get(pty)!;
			return join(set, fn, events, () => set.size === 0 && dropped.delete(pty));
		},
		/** The terminal host (re)connected: every terminal reattaches. */
		onReconnected(fn: () => void) {
			return join(reconnected, fn, reconnects);
		},
	};
}
