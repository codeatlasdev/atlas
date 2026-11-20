// GET /changelog/ and /pt-br/changelog/ -> the static page shell (nav, footer, styles from the last build) with the body swapped for the
// changelog of the LATEST published release (or of `?channel=beta|alpha`: the history at that channel's newest tag), read at the edge.
// The shell says which language it is: its words and its release stories travel inside it (see _lib/page.ts `swapChangelog`), so the
// Portuguese route (functions/pt-br/changelog/index.ts) is this same handler. If GitHub is down the last good copy is served; if there is
// none the static build's own body is served untouched (the offline fallback), so the page is never empty.
import { DEFAULT_CHANNEL } from "../_lib/channels";
import { renderChangelog } from "../_lib/changelog";
import { loadChangelog, resolveChannel } from "../_lib/edge";
import { localeRedirect } from "../_lib/locale";
import { swapChangelog } from "../_lib/page";
import { edgeHeaders, wantedChannel, wantsFresh } from "../_lib/respond";

const HTML = { "content-type": "text/html; charset=utf-8" };

export const onRequestGet: PagesFunction = async ({ request, next }) => {
	// A visitor who wants Portuguese gets /pt-br/changelog/ (see _lib/locale.ts; a /pt-br/ request is never redirected).
	const redirect = localeRedirect(request);
	if (redirect) return redirect;
	// `next()` serves the static asset for this path: the shell, and the offline fallback body between the markers.
	const html = await (await next()).text();
	try {
		// An unknown ?channel= is not an error for a page: it shows the default channel's changelog.
		const { value, stale } = await resolveChannel("changelog", wantedChannel(request.url) ?? DEFAULT_CHANNEL, loadChangelog, wantsFresh(request.url));
		if (!value) throw new Error("no published release");
		const swapped = swapChangelog(html, renderChangelog(value.markdown), { tag: value.tag, stale });
		if (swapped === null) return new Response(html, { headers: edgeHeaders(HTML, true) });
		return new Response(swapped, { headers: edgeHeaders({ ...HTML, "x-atlas-tag": value.tag }, stale) });
	} catch {
		return new Response(html, { headers: edgeHeaders(HTML, true) });
	}
};

// HEAD answers like GET (Pages routes methods separately); the body is dropped by the runtime.
export const onRequestHead = onRequestGet;
