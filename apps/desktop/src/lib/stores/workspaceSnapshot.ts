import type { Doc } from "#lib/stores/workspaceTypes.ts";

/**
 * `JSON.stringify(doc)` for the autosave, without walking the agent feed again on every change.
 *
 * The feed is the bulk of the doc (500 entries, ~500 KB on a used workspace) and is only ever replaced as a whole
 * (`ws.pushFeed`), never edited in place. Autosave runs inside the reactive flush of every selection, tab or layout
 * change; walking 500 reactive entries each time cost 1-2 ms of the main thread per space switch (#452). The feed's JSON
 * is kept while `doc.feed` is the same array, so a read of that one property is all the feed adds to the effect.
 */
export function docSerializer(): (doc: Doc) => string {
	let feed: unknown;
	let feedJson = "";
	return (doc) => {
		const { feed: f, ...rest } = doc;
		const head = JSON.stringify(rest);
		if (f === undefined) return head;
		if (f !== feed) {
			feed = f;
			feedJson = JSON.stringify(f);
		}
		return `${head.slice(0, -1)}${head === "{}" ? "" : ","}"feed":${feedJson}}`;
	};
}
