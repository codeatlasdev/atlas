// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { crewmateKey } from "./crewmateKey.ts";
import { isUnread, loadSeen, saveSeen } from "./unread.ts";

function memoryStorage(initial: Record<string, string> = {}) {
	const m = new Map(Object.entries(initial));
	return {
		get length() {
			return m.size;
		},
		key: (i: number) => [...m.keys()][i] ?? null,
		getItem: (k: string) => m.get(k) ?? null,
		setItem: (k: string, v: string) => void m.set(k, v),
	};
}

describe("saveSeen / loadSeen", () => {
	test("a saved timestamp is read back under `<server>:<crewmate>`", () => {
		const s = memoryStorage();
		saveSeen(s, "ssh:atlas-crew@box", "c1", 1_700_000_000_000);
		expect(s.getItem("crew:seen:ssh:atlas-crew@box:c1")).toBe("1700000000000");
		expect(loadSeen(s).get(crewmateKey("ssh:atlas-crew@box", "c1"))).toBe(1_700_000_000_000);
	});

	test("the local daemon has an empty server key", () => {
		const s = memoryStorage();
		saveSeen(s, "", "c1", 5);
		expect(loadSeen(s).get(crewmateKey("", "c1"))).toBe(5);
	});

	test("only crew:seen: keys are read; zero, negative and garbage values are dropped", () => {
		const s = memoryStorage({
			"crew:seen:a:1": "10",
			"crew:seen:nocolon": "7",
			"crew:seen:a:2": "0",
			"crew:seen:a:3": "-4",
			"crew:seen:a:4": "soon",
			"crew:favorite": "{}",
			"other:seen:a:1": "99",
		});
		expect([...loadSeen(s)]).toEqual([[crewmateKey("a", "1"), 10]]);
	});

	test("a storage that throws yields an empty map and a silent save", () => {
		const boom = {
			get length() {
				throw new Error("denied");
			},
			key: () => null,
			getItem: () => null,
			setItem: () => {
				throw new Error("full");
			},
		};
		expect(loadSeen(boom).size).toBe(0);
		expect(() => saveSeen(boom, "", "c1", 1)).not.toThrow();
	});
});

describe("isUnread", () => {
	test("a reply newer than the last seen one is unread", () => expect(isUnread(200, 100)).toBe(true));
	test("the same or an older reply is read", () => {
		expect(isUnread(100, 100)).toBe(false);
		expect(isUnread(50, 100)).toBe(false);
	});
	test("never seen counts as 0, so any reply is unread", () => expect(isUnread(1, undefined)).toBe(true));
	test("no reply yet is never unread", () => {
		expect(isUnread(undefined, undefined)).toBe(false);
		expect(isUnread(0, undefined)).toBe(false);
	});
});
