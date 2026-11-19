// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { docSerializer } from "./workspaceSnapshot.ts";
import { emptyDoc } from "./workspaceTypes.ts";

/** A feed entry that counts how many times the serializer reads it (what the autosave effect pays for, once per read). */
function entry(i: number, reads: { n: number }) {
	const e = { ts: i, worktreeId: "w", tabId: "t", agent: "omp", state: "idle" };
	return Object.defineProperty(e, "text", {
		enumerable: true,
		get() {
			reads.n++;
			return `entry ${i}`;
		},
	});
}

function docWithFeed(reads: { n: number }, size = 500) {
	const doc = emptyDoc();
	doc.selected = "a";
	doc.feed = Array.from({ length: size }, (_, i) => entry(i, reads));
	return doc;
}

test("the autosave string is the same document as JSON.stringify(doc)", () => {
	const doc = docWithFeed({ n: 0 }, 5);
	doc.recentWorktrees = ["a", "b"];
	expect(JSON.parse(docSerializer()(doc))).toEqual(JSON.parse(JSON.stringify(doc)));
});

// #452: the feed (500 entries, ~500 KB) was walked again by every autosave, inside the flush of every space switch.
test("a selection change does not walk the feed again", () => {
	const reads = { n: 0 };
	const doc = docWithFeed(reads);
	const serialize = docSerializer();
	serialize(doc);
	expect(reads.n).toBe(500);
	doc.selected = "b";
	doc.recentWorktrees = ["b", "a"];
	const next = serialize(doc);
	expect(reads.n).toBe(500);
	expect(JSON.parse(next).selected).toBe("b");
	expect(JSON.parse(next).feed).toHaveLength(500);
});

test("a new feed is serialized, an old one is not kept", () => {
	const reads = { n: 0 };
	const doc = docWithFeed(reads, 3);
	const serialize = docSerializer();
	serialize(doc);
	doc.feed = [entry(99, reads), ...doc.feed].slice(0, 3);
	const out = JSON.parse(serialize(doc));
	expect(out.feed.map((f) => f.ts)).toEqual([99, 0, 1]);
});

test("a doc without a feed, and a doc that is only a feed, stay valid JSON", () => {
	const serialize = docSerializer();
	const bare = { ...emptyDoc() };
	delete bare.feed;
	expect("feed" in JSON.parse(serialize(bare))).toBe(false);
	expect(JSON.parse(serialize({ feed: [{ ts: 1 }] }))).toEqual({ feed: [{ ts: 1 }] });
	expect(JSON.parse(serialize({ feed: [] }))).toEqual({ feed: [] });
});
