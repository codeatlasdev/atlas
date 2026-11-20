// /llms-full.txt: every English docs page in sidebar order in one file (src/lib/llms.ts).
import type { APIRoute } from "astro";
import { loadNav, markdownOf } from "../lib/docs";
import { llmsFull } from "../lib/llms";

export const GET: APIRoute = async () => {
	const { groups, entries } = await loadNav();
	const byId = new Map(entries.map((e) => [e.id, e]));
	const pages = groups.flatMap((g) => g.items).map((p) => ({ slug: p.slug, markdown: markdownOf(byId.get(p.slug)!) }));
	return new Response(llmsFull(pages), { headers: { "content-type": "text/plain; charset=utf-8" } });
};
