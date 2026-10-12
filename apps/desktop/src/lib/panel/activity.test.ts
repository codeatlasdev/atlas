// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { bucket, counts, filterRows, fold, group, parse, span, unread } from "./activity.ts";

const T0 = new Date(2026, 9, 11, 19, 20, 29).getTime();
const st = (name, state, ts = T0) => ({ ts, kind: "state", text: `${name} → ${state}` });

describe("parse", () => {
	test("state, alert and tunnel events", () => {
		expect(parse(st("server", "running"))).toMatchObject({ service: "server", state: "running" });
		expect(parse({ ts: 1, kind: "alert", text: "api: exit 1: boom" })).toMatchObject({
			service: "api",
			message: "exit 1: boom",
		});
		expect(parse({ ts: 1, kind: "tunnel", text: "tunnel up" })).toMatchObject({ service: null });
	});
});

describe("fold", () => {
	test("a stop/start/run burst of one service becomes one restart row", () => {
		const rows = fold([
			st("server", "running"),
			st("server", "stopped", T0 + 1000),
			st("server", "starting", T0 + 2000),
			st("server", "running", T0 + 3000),
		]);
		expect(rows).toHaveLength(1);
		expect(rows[0]).toMatchObject({ title: "server restarted", type: "restart", count: 4, tone: "ok" });
	});

	test("several restarts inside one burst count the cycles", () => {
		const evs = [];
		for (let i = 0; i < 3; i++) {
			evs.push(st("server", "stopped", T0 + i * 3000), st("server", "running", T0 + i * 3000 + 1500));
		}
		expect(fold(evs)[0].title).toBe("server restarted 3×");
	});

	test("a service that ends down says so", () => {
		const rows = fold([
			st("db", "running"),
			st("db", "failed", T0 + 500),
			st("db", "starting", T0 + 900),
			st("db", "failed", T0 + 1200),
		]);
		expect(rows[0]).toMatchObject({ tone: "fail" });
	});

	test("starting → running is a plain transition with its path as detail", () => {
		const rows = fold([st("web", "starting"), st("web", "running", T0 + 4000)]);
		expect(rows[0]).toMatchObject({ title: "web → running", detail: "starting → running", type: "state" });
	});

	test("different services never fold together; newest row first", () => {
		const rows = fold([st("a", "running", T0), st("b", "running", T0 + 10)]);
		expect(rows.map((r) => r.service)).toEqual(["b", "a"]);
	});

	test("changes further apart than the burst window stay separate", () => {
		const rows = fold([st("a", "running", T0), st("a", "stopped", T0 + 5 * 60_000)]);
		expect(rows).toHaveLength(2);
	});

	test("repeated identical alerts collapse with a count", () => {
		const alert = (ts) => ({ ts, kind: "alert", text: "api: boom" });
		const rows = fold([alert(T0), alert(T0 + 1000), alert(T0 + 2000)]);
		expect(rows).toHaveLength(1);
		expect(rows[0]).toMatchObject({ count: 3, type: "error", tone: "fail", detail: "3 times" });
	});

	test("failed alone is an error row", () => {
		expect(fold([st("api", "failed")])[0]).toMatchObject({ type: "error", tone: "fail" });
	});
});

describe("action rows", () => {
	test("started is info, finished ok, failed is an error-toned action", () => {
		const a = (text, ts) => ({ ts, kind: "action", text });
		const rows = fold([
			a("Migrate: started", T0),
			a("Migrate: finished", T0 + 1000),
			a("Seed: failed", T0 + 2000),
		]);
		expect(rows.map((r) => [r.title, r.tone, r.type])).toEqual([
			["Seed: failed", "fail", "action"],
			["Migrate: finished", "ok", "action"],
			["Migrate: started", "info", "action"],
		]);
		expect(filterRows(rows, "errors")).toHaveLength(1);
	});
});

describe("filters", () => {
	const rows = fold([
		st("a", "running", T0),
		st("a", "stopped", T0 + 100),
		st("a", "running", T0 + 200),
		{ ts: T0 + 90_000, kind: "alert", text: "b: boom" },
		{ ts: T0 + 200_000, kind: "action", text: "Migrate: started" },
		st("c", "running", T0 + 300_000),
	]);
	test("counts per filter", () => {
		expect(counts(rows)).toEqual({ all: 4, errors: 1, restarts: 1, actions: 1 });
	});
	test("errors / restarts / actions select their rows", () => {
		expect(filterRows(rows, "errors").map((r) => r.service)).toEqual(["b"]);
		expect(filterRows(rows, "restarts").map((r) => r.service)).toEqual(["a"]);
		expect(filterRows(rows, "actions")).toHaveLength(1);
		expect(filterRows(rows, "all")).toBe(rows);
	});
});

describe("bucket and group", () => {
	test("labels", () => {
		const now = new Date(2026, 9, 11, 19, 30).getTime();
		expect(bucket(now - 10_000, now)).toBe("Just now");
		expect(bucket(now - 2 * 60_000, now)).toBe("1 min ago");
		expect(bucket(now - 7 * 60_000, now)).toBe("5 min ago");
		expect(bucket(now - 20 * 60_000, now)).toBe("15 min ago");
		expect(bucket(now - 40 * 60_000, now)).toBe("30 min ago");
		expect(bucket(now - 90 * 60_000, now)).toBe("Today 18:00");
		expect(bucket(new Date(2026, 9, 10, 21, 5).getTime(), now)).toBe("Yesterday 21:00");
	});
	test("rows with the same bucket share a group, in order", () => {
		const now = T0 + 30_000;
		const rows = fold([
			st("a", "running", T0),
			st("b", "running", T0 + 5000),
			st("c", "running", T0 - 10 * 60_000),
		]);
		const g = group(rows, now);
		expect(g.map((x) => x.label)).toEqual(["Just now", "5 min ago"]);
		expect(g[0].rows).toHaveLength(2);
	});
});

describe("unread and span", () => {
	test("counts rows newer than the last seen time and flags errors", () => {
		const rows = fold([st("a", "running", T0), { ts: T0 + 1000, kind: "alert", text: "a: x" }]);
		expect(unread(rows, T0 - 1)).toEqual({ n: 2, errors: 1 });
		expect(unread(rows, T0 + 1000)).toEqual({ n: 0, errors: 0 });
	});
	test("span formats short and long ranges", () => {
		expect(span(0, 12_000)).toBe("12s");
		expect(span(0, 180_000)).toBe("3m");
		expect(span(0, 3_900_000)).toBe("1h 5m");
	});
});
