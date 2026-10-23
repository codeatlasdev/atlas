// bun run desktop:mirror [--name N] [--port P]
// Live view of an agent instance, native layers included (GPU terminal, page stage, browser tabs): frames come from
// ScreenCaptureKit (`atlas desktop debug shot --window`), served as MJPEG on http://127.0.0.1:<port>. Open it in
// your browser to watch; click on it to click the app (DOM events through the debug bridge, never OS input).
// The instance stays on its private virtual display: nothing ever shows on your screen. Ctrl-C stops the mirror,
// not the instance. While a bench or heavy job holds scripts/heavy.sh's lock the mirror pauses, so it never
// skews what they measure.
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $ } from "bun";

const arg = (k: string, d: string) => (process.argv.includes(k) ? process.argv[process.argv.indexOf(k) + 1] : d);
const name = arg("--name", "lab");
const port = Number(arg("--port", "13452"));
const root = join(import.meta.dir, "..");
const atlas = join(root, "target/debug/atlas");
/**
 * Map a point on the mirrored image to page CSS px. The window uses an overlay title bar (the page fills the whole
 * window), so the image is the page scaled by imageWidth / innerWidth. (WKWebView reports outerHeight 0: never use it.)
 */
export function toPage(img: { w: number; h: number }, view: { w: number; h: number }, x: number, y: number) {
	const scale = img.w / view.w;
	return { x: Math.round(x / scale), y: Math.round(y / scale) };
}

/** Every route needs the run's token (URL `?t=`): the stream shows the instance's window and a click runs in it,
 * so neither another local process nor a web page that guesses the port may reach them. */
export function authorized(url: URL, token: string): boolean {
	const t = url.searchParams.get("t");
	return t !== null && t.length === token.length && t === token;
}

if (import.meta.main) {
const sock = ["/private/tmp", "/tmp"].map((d) => `${d}/atlas-agent-${name}/debug.sock`).find(existsSync);
if (!sock) {
	console.error(`no agent instance "${name}": start one with bun run desktop:agent -- --name ${name}`);
	process.exit(1);
}
// Only ever an agent instance's socket, never the user's own app bridge.
const env = { ...process.env, ATLAS_DEBUG_SOCK: sock };
const heavyLock = join(process.env.TMPDIR ?? "/tmp", `atlas-heavy-${process.getuid?.()}.lock`);
const token = crypto.randomUUID().replaceAll("-", "");
const dir = mkdtempSync(join(tmpdir(), "atlas-mirror-"));

async function frame(): Promise<Uint8Array | null> {
	const png = join(dir, "f.png");
	const jpg = join(dir, "f.jpg");
	const r = await $`${atlas} desktop debug shot ${png} --window`.env(env).nothrow().quiet();
	if (r.exitCode !== 0) return null;
	await $`sips -s format jpeg -s formatOptions 70 -Z 1600 ${png} --out ${jpg}`.nothrow().quiet();
	return existsSync(jpg) ? new Uint8Array(readFileSync(jpg)) : null;
}

async function evalPage(js: string) {
	const r = await $`${atlas} desktop debug eval ${js}`.env(env).nothrow().quiet();
	return r.exitCode === 0 ? JSON.parse(r.stdout.toString() || "null") : null;
}

let latest: Uint8Array | null = null;
let paused = "";
let watchers = 0;
(async function capture() {
	for (;;) {
		paused = existsSync(heavyLock) ? "paused: a heavy job (bench, gate) is running" : "";
		if (!paused && watchers > 0) latest = (await frame()) ?? latest;
		await Bun.sleep(paused || watchers === 0 ? 500 : 80);
	}
})();

const PAGE = `<!doctype html><meta charset=utf-8><title>Atlas mirror · ${name}</title>
<style>body{margin:0;background:#111;color:#aaa;font:13px system-ui}header{padding:6px 10px}img{display:block;max-width:100%;cursor:crosshair}</style>
<header>agent instance <b>${name}</b> · live · click to click the app · <span id=s></span></header>
<img id=v src="/stream?t=${token}">
<script>
const v=document.getElementById('v'),s=document.getElementById('s');
v.onclick=async e=>{const r=v.getBoundingClientRect();const k=v.naturalWidth/r.width;
 const res=await fetch('/click?t=${token}',{method:'POST',headers:{'content-type':'application/json'},
  body:JSON.stringify({x:(e.clientX-r.left)*k,y:(e.clientY-r.top)*k,w:v.naturalWidth,h:v.naturalHeight})});
 s.textContent=await res.text();};
setInterval(async()=>{s.textContent=(await (await fetch('/state?t=${token}')).text())||s.textContent},2000);
</script>`;

const server = Bun.serve({
	hostname: "127.0.0.1",
	port,
	async fetch(req) {
		const url = new URL(req.url);
		if (!authorized(url, token)) return new Response("forbidden", { status: 403 });
		if (url.pathname === "/") return new Response(PAGE, { headers: { "content-type": "text/html" } });
		if (url.pathname === "/state") return new Response(paused);
		if (url.pathname === "/click" && req.method === "POST") {
			// Same-origin only: the click runs in the instance.
			if (req.headers.get("origin") !== `http://127.0.0.1:${port}` && req.headers.get("origin") !== `http://localhost:${port}`)
				return new Response("forbidden", { status: 403 });
			const c = (await req.json()) as { x: number; y: number; w: number; h: number };
			const view = await evalPage("return {w: innerWidth, h: innerHeight}");
			if (!view) return new Response("instance did not answer");
			const p = toPage({ w: c.w, h: c.h }, view, c.x, c.y);
			const r = await evalPage(`return window.__atlasDebug.clickAt(${p.x}, ${p.y})`);
			return new Response(r?.clicked ? `clicked <${r.tag}> ${r.text}` : (r?.reason ?? "click failed"));
		}
		if (url.pathname === "/stream") {
			watchers++;
			const boundary = "atlasframe";
			let last: Uint8Array | null = null;
			const stream = new ReadableStream({
				async pull(ctl) {
					while (latest === last || !latest) await Bun.sleep(30);
					last = latest;
					ctl.enqueue(new TextEncoder().encode(`--${boundary}\r\nContent-Type: image/jpeg\r\nContent-Length: ${last.length}\r\n\r\n`));
					ctl.enqueue(last);
					ctl.enqueue(new TextEncoder().encode("\r\n"));
				},
				cancel() {
					watchers--;
				},
			});
			return new Response(stream, { headers: { "content-type": `multipart/x-mixed-replace; boundary=${boundary}` } });
		}
		return new Response("not found", { status: 404 });
	},
});
console.log(`Atlas mirror: http://127.0.0.1:${server.port}/?t=${token}  (agent instance ${name}; Ctrl-C stops the mirror only)`);
process.on("SIGINT", () => {
	rmSync(dir, { recursive: true, force: true });
	process.exit(0);
});
}
