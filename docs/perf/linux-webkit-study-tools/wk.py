#!/usr/bin/env python3
# wk.py [--policy always|never|demand] [--cache doc|web|none] [--url U] [--dump-gpu] [--seconds N] [--transparent]
import gi, sys, argparse
gi.require_version('Gtk','3.0'); gi.require_version('WebKit2','4.1')
from gi.repository import Gtk, WebKit2, GLib
ap=argparse.ArgumentParser()
ap.add_argument('--policy'); ap.add_argument('--cache'); ap.add_argument('--url',default='about:blank')
ap.add_argument('--html'); ap.add_argument('--dump-gpu',action='store_true'); ap.add_argument('--seconds',type=int,default=0)
ap.add_argument('--transparent',action='store_true'); ap.add_argument('--size',default='1280x820')
a=ap.parse_args()
ctx=WebKit2.WebContext.new_ephemeral()
if a.cache: ctx.set_cache_model({'doc':WebKit2.CacheModel.DOCUMENT_VIEWER,'web':WebKit2.CacheModel.WEB_BROWSER,'none':WebKit2.CacheModel.DOCUMENT_BROWSER}[a.cache])
v=WebKit2.WebView.new_with_context(ctx)
s=v.get_settings()
if a.policy: s.set_hardware_acceleration_policy({'always':WebKit2.HardwareAccelerationPolicy.ALWAYS,'never':WebKit2.HardwareAccelerationPolicy.NEVER,'demand':WebKit2.HardwareAccelerationPolicy.ON_DEMAND}[a.policy])
s.set_enable_webgl(True)
if a.transparent: v.set_background_color(__import__('gi').repository.Gdk.RGBA(0,0,0,0))
w=Gtk.Window(title='wk'); W,H=map(int,a.size.split('x')); w.set_default_size(W,H); w.add(v); w.show_all()
w.connect('destroy',Gtk.main_quit)
if a.html: v.load_html(open(a.html).read(),'file:///')
else: v.load_uri(a.url)
if a.dump_gpu:
    def done(wv,res):
        r=wv.run_javascript_finish(res); print(r.get_js_value().to_string()); Gtk.main_quit()
    def go():
        v.run_javascript("document.body.innerText",None,done); return False
    GLib.timeout_add(4000,go)
if a.seconds: GLib.timeout_add_seconds(a.seconds,lambda:(Gtk.main_quit(),False)[1])
Gtk.main()
