// Portuguese docs pages: `Accept: text/markdown` gets the page's markdown (see _lib/negotiate.ts); nothing else changes, and a /pt-br/ URL is
// never redirected by language.
import { markdownPathOf, wantsMarkdown } from "../../_lib/negotiate";

const handle: PagesFunction<{ ASSETS: Fetcher }> = async ({ request, next, env }) => {
	const md = wantsMarkdown(request.headers.get("accept")) ? markdownPathOf(new URL(request.url).pathname) : null;
	const res = md ? await env.ASSETS.fetch(new Request(new URL(md, request.url), request)) : await next();
	const out = new Response(res.body, res);
	if (md && res.ok) out.headers.set("content-type", "text/markdown; charset=utf-8");
	out.headers.append("vary", "Accept");
	return out;
};

export const onRequestGet = handle;
export const onRequestHead = handle;
