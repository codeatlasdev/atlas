// bun run desktop:mirror [--name N] [--port P]
// Live view of an agent instance, native layers included (GPU terminal, page stage, browser tabs): the app streams its
// window with ScreenCaptureKit at up to 30 fps as in-memory JPEGs (debug bridge `frame`), served as MJPEG on
// http://127.0.0.1:<port>. Open the printed URL to watch; click and type on it to drive the app (DOM events and pty
// writes through the debug bridge, never OS input).
// The instance stays on its private virtual display: nothing ever shows on your screen. Ctrl-C stops the mirror,
// not the instance. While a bench or heavy job holds scripts/heavy.sh's lock the mirror pauses, so it never
// skews what they measure.
import { existsSync } from "node:fs";
import { join } from "node:path";

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
	const arg = (k: string, d: string) => (process.argv.includes(k) ? process.argv[process.argv.indexOf(k) + 1] : d);
	const name = arg("--name", "lab");
	const port = Number(arg("--port", "13452"));
	const sock = ["/private/tmp", "/tmp"].map((d) => `${d}/atlas-agent-${name}/debug.sock`).find(existsSync);
	if (!sock) {
		console.error(`no agent instance "${name}": start one with bun run desktop:agent -- --name ${name}`);
		process.exit(1);
	}
	const token = crypto.randomUUID().replaceAll("-", "");
	const heavyLock = join(process.env.TMPDIR ?? "/tmp", `atlas-heavy-${process.getuid?.()}.lock`);

	/** One line-delimited JSON connection to the instance's bridge (only ever an agent instance's socket). */
	function bridge(path: string) {
		let buf = "";
		const waiting: ((v: { ok: boolean; result?: any; error?: string }) => void)[] = [];
		let conn: Promise<Awaited<ReturnType<typeof Bun.connect>>> | null = null;
		const open = () =>
			(conn ??= Bun.connect({
				unix: path,
				socket: {
					data(_s, d) {
						buf += d.toString();
						for (let nl = buf.indexOf("\n"); nl >= 0; nl = buf.indexOf("\n")) {
							const line = buf.slice(0, nl);
							buf = buf.slice(nl + 1);
							waiting.shift()?.(JSON.parse(line));
						}
					},
					close() {
						conn = null;
						for (const w of waiting.splice(0)) w({ ok: false, error: "the instance closed the bridge" });
					},
				},
			}));
		return async (req: object) => {
			const s = await open();
			const { promise, resolve } = Promise.withResolvers<{ ok: boolean; result?: any; error?: string }>();
			waiting.push(resolve);
			s.write(`${JSON.stringify(req)}\n`);
			return promise;
		};
	}
	// Frames on their own connection: a pull waits for the next picture and must not delay clicks and keys.
	const frames = bridge(sock);
	const actions = bridge(sock);
	const evalPage = async (js: string) => {
		const r = await actions({ cmd: "eval", js });
		return r.ok ? r.result : null;
	};

	let latest: Uint8Array | null = null;
	let seq = 0;
	let paused = "";
	let error = "";
	let watchers = 0;
	let shown = 0;
	(async function capture() {
		for (;;) {
			paused = existsSync(heavyLock) ? "paused: a heavy job (bench, gate) is running" : "";
			if (paused || watchers === 0) {
				await Bun.sleep(300);
				continue;
			}
			const r = await frames({ cmd: "frame", since: seq, wait_ms: 1000 }).catch((e) => ({ ok: false, error: String(e) }));
			if (r.ok && r.result.seq !== seq) {
				seq = r.result.seq;
				latest = Uint8Array.from(Buffer.from(r.result.jpeg, "base64"));
				error = "";
			} else if (!r.ok) {
				error = r.error ?? "no frame";
				await Bun.sleep(500);
			}
		}
	})();
	setInterval(() => {
		if (watchers > 0) process.stdout.write(`\r${shown} frames/s   `);
		shown = 0;
	}, 1000);

	const PAGE = `<!doctype html><meta charset=utf-8><title>Atlas mirror · ${name}</title>
<style>body{margin:0;background:#111;color:#aaa;font:13px system-ui}header{padding:6px 10px}img{display:block;max-width:100%;cursor:crosshair;outline:none}img:focus{box-shadow:0 0 0 2px #3d5bff}</style>
<header>agent instance <b>${name}</b> · click the picture to click the app, then type: keys go to the app · <span id=s></span></header>
<img id=v tabindex=0 src="/stream?t=${token}">
<script>
const v=document.getElementById('v'),s=document.getElementById('s'),T='${token}';
const post=(p,b)=>fetch(p+'?t='+T,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(b)}).then(r=>r.text()).then(t=>s.textContent=t);
v.onclick=e=>{v.focus();const r=v.getBoundingClientRect();const k=v.naturalWidth/r.width;
 post('/click',{x:(e.clientX-r.left)*k,y:(e.clientY-r.top)*k,w:v.naturalWidth,h:v.naturalHeight});};
v.onkeydown=e=>{e.preventDefault();post('/key',{key:e.key,ctrl:e.ctrlKey,alt:e.altKey,meta:e.metaKey,shift:e.shiftKey});};
v.onpaste=e=>{e.preventDefault();for(const ch of e.clipboardData.getData('text'))post('/key',{key:ch==='\\n'?'Enter':ch});};
setInterval(async()=>{const t=await (await fetch('/state?t='+T)).text();if(t)s.textContent=t},2000);
</script>`;

	const server = Bun.serve({
		hostname: "127.0.0.1",
		port,
		idleTimeout: 0,
		async fetch(req) {
			const url = new URL(req.url);
			if (!authorized(url, token)) return new Response("forbidden", { status: 403 });
			if (url.pathname === "/") return new Response(PAGE, { headers: { "content-type": "text/html" } });
			if (url.pathname === "/state") return new Response(paused || error);
			if (url.pathname === "/click" && req.method === "POST") {
				const c = (await req.json()) as { x: number; y: number; w: number; h: number };
				const view = await evalPage("return {w: innerWidth, h: innerHeight}");
				if (!view) return new Response("instance did not answer");
				const p = toPage({ w: c.w, h: c.h }, view, c.x, c.y);
				const r = await evalPage(`return window.__atlasDebug.clickAt(${p.x}, ${p.y})`);
				return new Response(r?.clicked ? `clicked <${r.tag}> ${r.text}` : (r?.reason ?? "click failed"));
			}
			if (url.pathname === "/key" && req.method === "POST") {
				const k = await req.json();
				const r = await evalPage(`return await window.__atlasDebug.pressKey(${JSON.stringify(k)})`);
				return new Response(r ? `${r.key} → ${r.to}` : "key failed");
			}
			if (url.pathname === "/stream") {
				watchers++;
				const boundary = "atlasframe";
				let last: Uint8Array | null = null;
				const enc = new TextEncoder();
				const stream = new ReadableStream({
					async pull(ctl) {
						while (latest === last || !latest) await Bun.sleep(5);
						last = latest;
						shown++;
						ctl.enqueue(enc.encode(`--${boundary}\r\nContent-Type: image/jpeg\r\nContent-Length: ${last.length}\r\n\r\n`));
						ctl.enqueue(last);
						ctl.enqueue(enc.encode("\r\n"));
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
}
