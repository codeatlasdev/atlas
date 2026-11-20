// /llms.txt: generated from docs/_nav.json (src/lib/llms.ts). There is no static copy in public/.
import type { APIRoute } from "astro";
import { llmsGroups } from "../lib/docs";
import { llmsTxt } from "../lib/llms";

export const GET: APIRoute = async () => new Response(llmsTxt(await llmsGroups("en"), "en"), { headers: { "content-type": "text/plain; charset=utf-8" } });
