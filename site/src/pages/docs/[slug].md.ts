// /docs/<slug>.md: the raw markdown of every docs page, for agents (advertised by <link rel="alternate" type="text/markdown"> and served
// for `Accept: text/markdown` by functions/docs/[[path]].ts).
import type { APIRoute, GetStaticPaths } from "astro";
import { loadNav, markdownOf } from "../../lib/docs";

export const getStaticPaths = (async () => {
	const { entries } = await loadNav();
	return entries.map((entry) => ({ params: { slug: entry.id }, props: { markdown: markdownOf(entry) } }));
}) satisfies GetStaticPaths;

export const GET: APIRoute = ({ props }) => new Response(props.markdown, { headers: { "content-type": "text/markdown; charset=utf-8" } });
