// /pt-br/llms.txt: the docs that have a Portuguese translation (src/lib/llms.ts).
import type { APIRoute } from "astro";
import { llmsGroups } from "../../lib/docs";
import { llmsTxt } from "../../lib/llms";

export const GET: APIRoute = async () => new Response(llmsTxt(await llmsGroups("pt-BR"), "pt-BR"), { headers: { "content-type": "text/plain; charset=utf-8" } });
