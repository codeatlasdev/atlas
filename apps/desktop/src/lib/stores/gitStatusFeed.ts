/**
 * One `git status` per worktree, however many files change.
 *
 * Every `fs_event` from every watcher used to run `git_status` at once (several git processes each), in the Source
 * Control panel and again in the Explorer; on Linux (inotify) a build or `npm install` sent thousands of events and
 * the app spent itself spawning git (#307). Here events are filtered to the worktree, debounced (trailing), and run
 * single-flight: one status in flight, at most one queued behind it. Both panels read the same result.
 *
 * Pure (Tauri's `listen`, the fetch and the timers are passed in), so the scheduling is testable.
 */
import type { GitStatus } from "#lib/api.ts";

type Listen = (name: string, cb: (e: { payload: unknown }) => void) => Promise<() => void>;

export interface Timers {
	now(): number;
	setTimeout(fn: () => void, ms: number): unknown;
	clearTimeout(id: unknown): void;
}

/** `.git` entries whose change moves the status: the index (stage/unstage), HEAD (checkout/commit) and refs. */
const GIT_STATE = /^\.git\/(index|HEAD|refs\/.*)$/;

/** Does an `fs_event` batch change the status of the worktree at `root`? Other folders and `.git` noise do not. */
export function touchesStatus(root: string, paths: string[] | undefined): boolean {
	if (!root) return false;
	const base = root.replace(/\/+$/, "") + "/";
	return (paths ?? []).some((p) => {
		if (!p.startsWith(base)) return false;
		const rel = p.slice(base.length);
		return rel !== ".git" && (!rel.startsWith(".git/") || GIT_STATE.test(rel));
	});
}

export interface Coalescer {
	/** Debounced: run after `delay` ms without another request. */
	request(): void;
	/** Run now (or queue the one rerun); resolves when a run that started after this call has finished. */
	now(): Promise<void>;
	dispose(): void;
}

/**
 * Runs `run` for requests: `request()` is a trailing debounce of `delay` ms (a burst is one run; `maxWait` keeps a
 * build that never goes quiet refreshing); `now()` skips the debounce. Never two runs at once: a request that comes
 * due while one is in flight sets ONE rerun, however many there are. `now()` resolves when a run that started after
 * the call has finished.
 */
export function createCoalescer(
	run: () => Promise<void>,
	env: Timers & { delay: number; maxWait: number },
): Coalescer {
	let timer: unknown;
	let deadline: number | undefined;
	let inflight = false;
	let rerun = false;
	let waiters: Array<() => void> = [];

	function clear() {
		if (timer !== undefined) env.clearTimeout(timer);
		timer = deadline = undefined;
	}

	async function cycle() {
		inflight = true;
		try {
			do {
				rerun = false;
				const covered = waiters;
				waiters = [];
				try {
					await run();
				} finally {
					for (const done of covered) done();
				}
			} while (rerun);
		} finally {
			inflight = false;
		}
	}

	function now(): Promise<void> {
		clear();
		const { promise, resolve } = Promise.withResolvers<void>();
		waiters.push(resolve);
		if (inflight) rerun = true;
		else void cycle();
		return promise;
	}

	return {
		request() {
			const t = env.now();
			deadline ??= t + env.maxWait;
			if (timer !== undefined) env.clearTimeout(timer);
			timer = env.setTimeout(() => void now(), Math.min(env.delay, Math.max(0, deadline - t)));
		},
		now,
		dispose: clear,
	};
}

export interface StatusSink {
	status(s: GitStatus): void;
	error(e: unknown): void;
}

interface Entry {
	root: string;
	sinks: Set<StatusSink>;
	last?: GitStatus;
	co: Coalescer;
}

/** Status per worktree, fed to every subscriber, refreshed by the page's one host-event listener. */
export function createGitStatusFeed(
	env: Timers & {
		fetch(id: string): Promise<GitStatus>;
		listen: Listen;
		delay: number;
		maxWait: number;
	},
) {
	const entries = new Map<string, Entry>();
	let off: Promise<() => void> | undefined;

	function onHostEvent(p: unknown) {
		const ev = p as { ev?: string; paths?: string[] } | null;
		if (ev?.ev !== "fs_event") return;
		for (const e of entries.values()) if (touchesStatus(e.root, ev.paths)) e.co.request();
	}

	function entry(id: string, root: string): Entry {
		const e: Entry = {
			root,
			sinks: new Set(),
			co: createCoalescer(async () => {
				try {
					const s = await env.fetch(id);
					e.last = s;
					for (const k of [...e.sinks]) k.status(s);
				} catch (err) {
					for (const k of [...e.sinks]) k.error(err);
				}
			}, env),
		};
		return e;
	}

	return {
		/** Feed `sink` the status of worktree `id` (its checkout at `root`); the returned off stops it. */
		watch(id: string, root: string, sink: StatusSink): () => void {
			let e = entries.get(id);
			const fresh = !e;
			if (!e) entries.set(id, (e = entry(id, root)));
			e.sinks.add(sink);
			if (entries.size === 1 && !off) off = env.listen("host://event", (ev) => onHostEvent(ev.payload));
			if (e.last) sink.status(e.last);
			if (fresh) void e.co.now();
			const mine = e;
			return () => {
				mine.sinks.delete(sink);
				if (mine.sinks.size > 0) return;
				mine.co.dispose();
				entries.delete(id);
				if (entries.size > 0) return;
				// The answer may still be on its way (a fast mount/unmount): release whatever it brings.
				const pending = off;
				off = undefined;
				void pending?.then((u) => u());
			};
		},
		/** A status now (after a stage or commit, a poll, a reconnect): single-flight, resolves once it is delivered. */
		refresh(id: string): Promise<void> {
			return entries.get(id)?.co.now() ?? Promise.resolve();
		},
	};
}
