// Content negotiation of the docs pages: `GET /docs/<slug>/` with `Accept: text/markdown` answers with the page's markdown
// (/docs/<slug>.md) instead of HTML, so an agent that asks for markdown never has to scrape the page. Pure, so bun test pins the rules.

interface Range {
	q: number;
	/** Position in the header: earlier wins a tie. */
	index: number;
	/** `text/html` and `text/markdown` named outright, as opposed to matched by a wildcard range. */
	exact: boolean;
}

/** The quality and position of the best Accept entry that matches `type`; undefined when none does (or all say q=0). */
function rangeFor(accept: string, type: "text/html" | "text/markdown"): Range | undefined {
	let best: Range | undefined;
	accept.split(",").forEach((item, index) => {
		const [range, ...params] = item.trim().toLowerCase().split(";");
		const kind = range.trim();
		const exact = kind === type;
		if (!exact && kind !== "text/*" && kind !== "*/*") return;
		const qp = params.map((p) => p.trim()).find((p) => p.startsWith("q="));
		const q = qp ? Number(qp.slice(2)) : 1;
		if (!Number.isFinite(q) || q <= 0) return;
		// An exact match beats a wildcard whatever its q (RFC 9110, most specific range wins), then the higher q
		if (!best || (exact && !best.exact) || (exact === best.exact && q > best.q)) best = { q, index, exact };
	});
	return best;
}

// Whether the client prefers markdown to HTML. A browser sends text/html first and a wildcard at q=0.8; a bare wildcard (curl, most
// fetchers) means HTML too. Markdown wins only when it is named, and either ranks above HTML or ties with it and comes first in the header.
export function wantsMarkdown(accept: string | null | undefined): boolean {
	if (!accept) return false;
	const md = rangeFor(accept, "text/markdown");
	if (!md?.exact) return false;
	const html = rangeFor(accept, "text/html");
	if (!html) return true;
	if (md.q !== html.q) return md.q > html.q;
	return !html.exact || md.index < html.index;
}

/** `/docs/x/`, `/docs/x` or `/pt-br/docs/x/` to the path of its markdown (`/docs/x.md`); null for any other path (the docs index, a file). */
export function markdownPathOf(pathname: string): string | null {
	const m = pathname.match(/^(\/pt-br)?\/docs\/([a-z0-9-]+)\/?$/);
	return m ? `${m[1] ?? ""}/docs/${m[2]}.md` : null;
}
