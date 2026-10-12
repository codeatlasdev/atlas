// bun run og: renders public/og.png (1200x630) from the real hero board with the logo tile.
// Run from a folder where `playwright` is installed (it is not a site dependency): cp this file there or use `bun --cwd`.
import { chromium } from "playwright";
import { resolve, join, extname } from "node:path";

const root = resolve(import.meta.dir, "..");
const dist = join(root, "dist");
const types: Record<string, string> = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".jpg": "image/jpeg", ".png": "image/png", ".webp": "image/webp", ".ico": "image/x-icon" };
const server = Bun.serve({
	port: 0,
	async fetch(req) {
		let p = decodeURIComponent(new URL(req.url).pathname);
		if (p.endsWith("/")) p += "index.html";
		const f = Bun.file(join(dist, p));
		return (await f.exists()) ? new Response(f, { headers: { "content-type": types[extname(p)] ?? "application/octet-stream" } }) : new Response("nf", { status: 404 });
	},
});
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, reducedMotion: "reduce" });
await page.goto(`http://127.0.0.1:${server.port}/`, { waitUntil: "load" });
await page.addStyleTag({ content: ".nav, .note, .ask, .inst, .tag { display: none !important } .hero { padding-top: 2.6rem !important } h1 { font-size: 4.3rem !important } .copy::before { content: \"\"; display: block; width: 84px; height: 84px; margin-bottom: 1.2rem; border-radius: 18px; background: #fff url(/favicon.svg) center / 70% no-repeat; }" });
await page.waitForTimeout(500);
await page.screenshot({ path: `${root}/public/og.png`, type: "png" });
await browser.close();
server.stop();
console.log("ok og.png (apple-touch-icon and favicon.ico are generated from assets/logo.svg, see README)");
