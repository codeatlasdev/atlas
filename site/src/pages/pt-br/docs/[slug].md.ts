// /pt-br/docs/<slug>.md: the translation's markdown when the page is translated, else the English page (like the HTML page, which shows
// the English body under /pt-br/ with a banner).
import type { APIRoute, GetStaticPaths } from "astro";
import { loadNav, markdownOf } from "../../../lib/docs";

export const getStaticPaths = (async () => {
	const { entries, translations } = await loadNav("pt-BR");
	return entries.map((entry) => ({ params: { slug: entry.id }, props: { markdown: markdownOf(entry, translations.get(entry.id)!.translation) } }));
}) satisfies GetStaticPaths;

export const GET: APIRoute = ({ props }) => new Response(props.markdown, { headers: { "content-type": "text/markdown; charset=utf-8" } });
