// English docs pages: a visitor who wants Portuguese (choice, country, browser) is redirected to the pt-BR twin (see _lib/locale.ts), and a
// client that asks for `Accept: text/markdown` gets the page's markdown (see _lib/negotiate.ts). Every other request is the static page.
// The answer depends on Accept, so every response carries `Vary: Accept` (the locale redirect adds its own Vary).
import { localeRedirect } from "../_lib/locale";
import { markdownPathOf, wantsMarkdown } from "../_lib/negotiate";

const handle: PagesFunction<{ ASSETS: Fetcher }> = async ({ request, next, env }) => {
	const { pathname } = new URL(request.url);
	const wants = wantsMarkdown(request.headers.get("accept"));
	// The language redirect is for pages. Whoever asks for the markdown (a .md address, or Accept) gets the English file they named.
	const redirect = wants || pathname.endsWith(".md") ? null : localeRedirect(request);
	if (redirect) return redirect;
	const md = wants ? markdownPathOf(pathname) : null;
	const res = md ? await env.ASSETS.fetch(new Request(new URL(md, request.url), request)) : await next();
	const out = new Response(res.body, res);
	if (md && res.ok) out.headers.set("content-type", "text/markdown; charset=utf-8");
	out.headers.append("vary", "Cookie, Accept-Language, Accept");
	return out;
};

export const onRequestGet = handle;
export const onRequestHead = handle;
