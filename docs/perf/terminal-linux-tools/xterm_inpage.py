#!/usr/bin/env python3
# xterm_inpage.py <corpus.bin> <acked|bulk> [--cols 100 --rows 30] -> one JSON line: MB/s and web-process CPU per MB.
# xterm.js 6.0.0 + addon-webgl 0.19.0 from apps/desktop/node_modules in a plain GTK3 WebKitGTK window (no Tauri, no IPC):
# the corpus is fetched by the page from a local server and written straight into term.write. acked = 64 KiB chunks, each
# awaiting write's callback (what ptyStream.ts does); bulk = 1 MiB chunks, a callback every fourth.
# Run through inenv.sh (private sway). Web-process CPU = utime+stime of the WebKitWebProcess children of this process.
import argparse, http.server, json, os, sys, threading, time, glob
import gi
gi.require_version('Gtk', '3.0'); gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2, GLib
ap = argparse.ArgumentParser(); ap.add_argument('corpus'); ap.add_argument('mode', choices=['acked', 'bulk'])
ap.add_argument('--cols', type=int, default=100); ap.add_argument('--rows', type=int, default=30)
ap.add_argument('--xterm', default=os.environ.get('XTERM_DIR'), required='XTERM_DIR' not in os.environ, help='apps/desktop/node_modules/@xterm (or $XTERM_DIR); inenv.sh changes HOME, so there is no ~ default')
a = ap.parse_args()
corpus = os.path.abspath(a.corpus)
PAGE = f"""<!doctype html><meta charset=utf-8><style>html,body{{margin:0;background:#111}}#t{{width:900px;height:560px}}</style>
<div id=t></div><script src=/x/xterm/lib/xterm.js></script><script src=/x/addon-webgl/lib/addon-webgl.js></script><script>
(async () => {{
  const term = new Terminal({{cols: {a.cols}, rows: {a.rows}, scrollback: 1000, allowProposedApi: true}});
  term.open(document.getElementById('t')); term.loadAddon(new WebglAddon.WebglAddon());
  const buf = new Uint8Array(await (await fetch('/corpus')).arrayBuffer());
  await fetch('/start'); const t0 = performance.now();
  const step = '{a.mode}' === 'acked' ? 65536 : 1 << 20;
  let n = 0;
  for (let i = 0; i < buf.length; i += step, n++) {{
    const chunk = buf.subarray(i, i + step);
    if ('{a.mode}' === 'acked' || n % 4 === 3) await new Promise(r => term.write(chunk, r)); else term.write(chunk);
  }}
  await new Promise(r => term.write('', r));
  const ms = performance.now() - t0;
  await fetch('/done?ms=' + ms + '&bytes=' + buf.length);
}})();
</script>"""
state = {}
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *x): pass
    def do_GET(self):
        p = self.path.split('?')[0]
        if p == '/': body, ct = PAGE.encode(), 'text/html'
        elif p == '/corpus': body, ct = open(corpus, 'rb').read(), 'application/octet-stream'
        elif p.startswith('/x/'):
            body, ct = open(os.path.join(a.xterm, p[3:]), 'rb').read(), 'text/javascript'
        elif p == '/start': state['cpu0'] = web_cpu(); state['t0'] = time.time(); body, ct = b'ok', 'text/plain'
        elif p == '/done':
            q = dict(x.split('=') for x in self.path.split('?')[1].split('&'))
            ms, nb = float(q['ms']), int(q['bytes']); cpu = web_cpu() - state['cpu0']
            state['out'] = dict(corpus=os.path.basename(corpus), mode=a.mode, mb=round(nb / 1e6, 1), mbps=round(nb / 1e3 / ms, 1),
                                web_cpu_s=round(cpu, 2), web_cpu_ms_per_mb=round(cpu * 1000 / (nb / 1e6), 1),
                                load1=os.getloadavg()[0], web_pss_mb=round(web_pss() / 1024, 0))
            body, ct = b'ok', 'text/plain'; GLib.idle_add(Gtk.main_quit)
        else: self.send_error(404); return
        self.send_response(200); self.send_header('Content-Type', ct); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
def web_pids():
    me = os.getpid(); out = []
    for d in glob.glob('/proc/[0-9]*'):
        try:
            if int(open(d + '/stat').read().rsplit(')', 1)[1].split()[1]) == me or True:
                if 'WebKitWebProcess' in open(d + '/cmdline').read(): out.append(d)
        except Exception: pass
    return out
def web_cpu():
    t = 0.0
    for d in web_pids():
        f = open(d + '/stat').read().rsplit(')', 1)[1].split(); t += (int(f[11]) + int(f[12])) / os.sysconf('SC_CLK_TCK')
    return t
def web_pss():
    k = 0
    for d in web_pids():
        for l in open(d + '/smaps_rollup'):
            if l.startswith('Pss:'): k += int(l.split()[1])
    return k
srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), H); threading.Thread(target=srv.serve_forever, daemon=True).start()
ctx = WebKit2.WebContext.new_ephemeral(); v = WebKit2.WebView.new_with_context(ctx); v.get_settings().set_enable_webgl(True)
w = Gtk.Window(title='xterm-inpage'); w.set_default_size(960, 620); w.add(v); w.show_all()
v.load_uri(f'http://127.0.0.1:{srv.server_address[1]}/')
GLib.timeout_add_seconds(600, Gtk.main_quit); Gtk.main()
print(json.dumps(state.get('out', {'error': 'no result'})))
