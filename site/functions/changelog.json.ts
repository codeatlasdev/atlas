// GET /changelog.json[?channel=stable|beta|alpha] -> the public changelog at the channel's newest tag (default: the default channel, alpha until a real Stable exists) as data:
// { tag, entries: [{ version, date, id, html, kind, superseded }] }. The markdown at a channel's newest tag carries the whole history.
import { renderChangelog } from "./_lib/changelog";
import { loadChangelog, resolveChannel } from "./_lib/edge";
import { badChannel, edgeHeaders, wantedChannel, wantsFresh } from "./_lib/respond";

export const onRequestGet: PagesFunction = async ({ request }) => {
	const channel = wantedChannel(request.url);
	if (!channel) return badChannel();
	try {
		const { value, stale } = await resolveChannel("changelog", channel, loadChangelog, wantsFresh(request.url));
		if (!value) throw new Error(`no published ${channel} release`);
		// The documented shape only: the page-only pieces (summary, details, counts) are not part of the data contract.
		const entries = renderChangelog(value.markdown).entries.map(({ version, date, id, html, kind, superseded }) => ({ version, date, id, html, kind, superseded }));
		return Response.json({ tag: value.tag, entries }, { headers: edgeHeaders({ "access-control-allow-origin": "*" }, stale) });
	} catch (err) {
		return Response.json({ error: "changelog_unavailable", detail: String(err) }, { status: 502, headers: { "cache-control": "no-store" } });
	}
};

// HEAD answers like GET (Pages routes methods separately); the body is dropped by the runtime.
export const onRequestHead = onRequestGet;
