// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { createCoalescer, createGitStatusFeed, touchesStatus } from "./gitStatusFeed.ts";

// Incident #307 (Linux, inotify): every `fs_event` from any watcher ran `git status` at once, so a build or
// `npm install` spawned thousands of git processes (each status is several) and starved the app.

/** Manual clock: timers fire only when the test advances time. */
function fakeClock() {
	let t = 0;
	let seq = 0;
	const timers = new Map<number, { at: number; fn: () => void }>();
	return {
		now: () => t,
		setTimeout: (fn: () => void, ms: number) => {
			timers.set(++seq, { at: t + ms, fn });
			return seq;
		},
		clearTimeout: (id: number) => void timers.delete(id),
		async advance(ms: number) {
			const end = t + ms;
			for (;;) {
				const due = [...timers].filter(([, v]) => v.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
				if (!due) break;
				timers.delete(due[0]);
				t = due[1].at;
				due[1].fn();
				await tick();
			}
			t = end;
		},
		pending: () => timers.size,
	};
}
const tick = async () => {
	for (let i = 0; i < 10; i++) await Promise.resolve();
};
/** A run that finishes when the test says so. */
function gate() {
	const runs: Array<() => void> = [];
	return {
		runs,
		run: () => new Promise<void>((res) => runs.push(res)),
		finish: async (i = runs.length - 1) => {
			runs[i]();
			await tick();
		},
	};
}

describe("coalescer", () => {
	test("a burst of events is one run, after the trailing debounce", async () => {
		const clock = fakeClock();
		let n = 0;
		const co = createCoalescer(async () => void n++, { delay: 250, maxWait: 2000, ...clock });
		for (let i = 0; i < 500; i++) {
			co.request();
			await clock.advance(1);
		}
		expect(n).toBe(0); // still inside the burst: the timer keeps moving
		await clock.advance(248); // the last event was 1 ms ago
		expect(n).toBe(0);
		await clock.advance(1);
		expect(n).toBe(1);
		await clock.advance(10_000);
		expect(n).toBe(1);
	});

	test("events during a run cause exactly one rerun, never concurrent runs", async () => {
		const clock = fakeClock();
		const g = gate();
		let live = 0;
		let max = 0;
		const co = createCoalescer(
			async () => {
				live++;
				max = Math.max(max, live);
				await g.run();
				live--;
			},
			{ delay: 250, maxWait: 2000, ...clock },
		);
		co.request();
		await clock.advance(250);
		expect(g.runs.length).toBe(1);
		for (let i = 0; i < 40; i++) {
			co.request();
			await clock.advance(30); // keeps firing the debounce while run #1 is still in flight
		}
		await clock.advance(1000);
		expect(g.runs.length).toBe(1);
		await g.finish(0);
		expect(g.runs.length).toBe(2);
		await g.finish(1);
		await clock.advance(5000);
		expect(g.runs.length).toBe(2);
		expect(max).toBe(1);
	});

	test("an event after the run finished runs again later", async () => {
		const clock = fakeClock();
		let n = 0;
		const co = createCoalescer(async () => void n++, { delay: 250, maxWait: 2000, ...clock });
		co.request();
		await clock.advance(250);
		co.request();
		await clock.advance(250);
		expect(n).toBe(2);
	});

	test("a build that never goes quiet still refreshes every maxWait", async () => {
		const clock = fakeClock();
		let n = 0;
		const co = createCoalescer(async () => void n++, { delay: 250, maxWait: 2000, ...clock });
		for (let i = 0; i < 200; i++) {
			co.request();
			await clock.advance(100); // 20 s of events 100 ms apart
		}
		expect(n).toBe(10);
	});

	test("now() skips the debounce, joins a run in flight with one rerun, and resolves once covered", async () => {
		const clock = fakeClock();
		const g = gate();
		const co = createCoalescer(() => g.run(), { delay: 250, maxWait: 2000, ...clock });
		const a = co.now();
		const b = co.now();
		const c = co.now();
		expect(g.runs.length).toBe(1);
		let done = 0;
		for (const p of [a, b, c]) void p.then(() => done++);
		await g.finish(0);
		expect(done).toBe(1); // `a` started before b and c asked
		expect(g.runs.length).toBe(2);
		await g.finish(1);
		expect(done).toBe(3);
		expect(g.runs.length).toBe(2);
	});

	test("dispose drops a pending debounce", async () => {
		const clock = fakeClock();
		let n = 0;
		const co = createCoalescer(async () => void n++, { delay: 250, maxWait: 2000, ...clock });
		co.request();
		co.dispose();
		await clock.advance(5000);
		expect(n).toBe(0);
	});
});

describe("touchesStatus", () => {
	const root = "/home/u/proj";
	test("only paths inside the worktree count", () => {
		expect(touchesStatus(root, [`${root}/src/a.ts`])).toBe(true);
		expect(touchesStatus(root, ["/home/u/other/src/a.ts"])).toBe(false);
		expect(touchesStatus(root, ["/home/u/proj-two/a.ts"])).toBe(false); // prefix of the name, not of the path
		expect(touchesStatus(root, [])).toBe(false);
		expect(touchesStatus(root, undefined)).toBe(false);
		expect(touchesStatus(`${root}/`, [`${root}/a.ts`])).toBe(true);
	});
	test(".git internals only count when they change status", () => {
		for (const p of [".git/index", ".git/HEAD", ".git/refs/heads/main", ".git/refs/remotes/origin/main"])
			expect(touchesStatus(root, [`${root}/${p}`])).toBe(true);
		for (const p of [
			".git",
			".git/index.lock",
			".git/objects/ab/cdef",
			".git/logs/HEAD",
			".git/FETCH_HEAD",
			".git/hooks/x",
		])
			expect(touchesStatus(root, [`${root}/${p}`])).toBe(false);
		expect(touchesStatus(root, [`${root}/.github/ci.yml`, `${root}/.gitignore`])).toBe(true);
	});
	test("one relevant path in a batch is enough", () => {
		expect(touchesStatus(root, [`${root}/.git/objects/x`, `${root}/a.ts`])).toBe(true);
	});
});

/** A fake Tauri `listen` with registration counts. */
function fakeTauri() {
	const handlers = new Set<(e: { payload: unknown }) => void>();
	const t = {
		listens: 0,
		unlistens: 0,
		listen: async (_name: string, cb: (e: { payload: unknown }) => void) => {
			t.listens++;
			handlers.add(cb);
			return () => {
				t.unlistens++;
				handlers.delete(cb);
			};
		},
		emit: (payload: unknown) => handlers.forEach((cb) => cb({ payload })),
	};
	return t;
}
const fsEvent = (paths: string[], id = "w1") => ({ ev: "fs_event", id, paths, kind: "modify" });
const status = (n: number) => ({
	branch: `b${n}`,
	upstream: null,
	ab: null,
	base: "HEAD",
	vs_base: null,
	entries: [],
	operation: null,
});

describe("status feed", () => {
	function setup() {
		const clock = fakeClock();
		const tauri = fakeTauri();
		const fetched: string[] = [];
		const feed = createGitStatusFeed({
			fetch: async (id: string) => {
				fetched.push(id);
				return status(fetched.length);
			},
			listen: tauri.listen,
			delay: 250,
			maxWait: 2000,
			...clock,
		});
		return { clock, tauri, fetched, feed };
	}

	test("500 fs events in a burst are one git status; before #307 they were 500", async () => {
		const { clock, tauri, fetched, feed } = setup();
		const seen: unknown[] = [];
		feed.watch("wt", "/r/wt", { status: (s) => seen.push(s), error: () => {} });
		await tick();
		await clock.advance(0);
		const atMount = fetched.length;
		for (let i = 0; i < 500; i++) {
			tauri.emit(fsEvent([`/r/wt/node_modules/p${i}/index.js`]));
			await clock.advance(2);
		}
		await clock.advance(250);
		expect(fetched.length - atMount).toBe(1);
		expect(seen.at(-1)).toEqual(status(fetched.length));
	});

	test("events of another worktree, other event kinds and .git noise start nothing", async () => {
		const { clock, tauri, fetched, feed } = setup();
		feed.watch("wt", "/r/wt", { status: () => {}, error: () => {} });
		await tick();
		const atMount = fetched.length;
		tauri.emit(fsEvent(["/r/other/a.ts"]));
		tauri.emit({ ev: "pty_output", id: "w1", paths: ["/r/wt/a.ts"] });
		tauri.emit(fsEvent(["/r/wt/.git/objects/aa/bb"]));
		tauri.emit(null);
		await clock.advance(5000);
		expect(fetched.length).toBe(atMount);
	});

	test("two panels on one worktree share one status and one host listener", async () => {
		const { clock, tauri, fetched, feed } = setup();
		const a: unknown[] = [];
		const b: unknown[] = [];
		const offA = feed.watch("wt", "/r/wt", { status: (s) => a.push(s), error: () => {} });
		const offB = feed.watch("wt", "/r/wt", { status: (s) => b.push(s), error: () => {} });
		await tick();
		expect(tauri.listens).toBe(1);
		const atMount = fetched.length;
		tauri.emit(fsEvent(["/r/wt/a.ts"]));
		await clock.advance(250);
		expect(fetched.length - atMount).toBe(1);
		expect(a.at(-1)).toEqual(b.at(-1));
		offA();
		tauri.emit(fsEvent(["/r/wt/a.ts"]));
		await clock.advance(250);
		expect(b.at(-1)).toEqual(status(fetched.length)); // B still fed
		expect(tauri.unlistens).toBe(0);
		offB();
		await tick();
		expect(tauri.unlistens).toBe(1);
		const after = fetched.length;
		tauri.emit(fsEvent(["/r/wt/a.ts"]));
		await clock.advance(5000);
		expect(fetched.length).toBe(after);
	});

	test("a failing status reaches every sink as an error and the next event still works", async () => {
		const clock = fakeClock();
		const tauri = fakeTauri();
		let fail = true;
		const feed = createGitStatusFeed({
			fetch: async () => {
				if (fail) throw "git: boom";
				return status(1);
			},
			listen: tauri.listen,
			delay: 250,
			maxWait: 2000,
			...clock,
		});
		const errs: unknown[] = [];
		const oks: unknown[] = [];
		feed.watch("wt", "/r/wt", { status: (s) => oks.push(s), error: (e) => errs.push(e) });
		await tick();
		expect(errs).toEqual(["git: boom"]);
		fail = false;
		tauri.emit(fsEvent(["/r/wt/a.ts"]));
		await clock.advance(250);
		expect(oks).toEqual([status(1)]);
	});

	test("refresh() is an immediate single-flight run that resolves with the status delivered", async () => {
		const { fetched, feed } = setup();
		const seen: unknown[] = [];
		feed.watch("wt", "/r/wt", { status: (s) => seen.push(s), error: () => {} });
		await tick();
		const before = fetched.length;
		await Promise.all([feed.refresh("wt"), feed.refresh("wt")]);
		expect(fetched.length - before).toBeLessThanOrEqual(2);
		expect(seen.at(-1)).toEqual(status(fetched.length));
	});
});
