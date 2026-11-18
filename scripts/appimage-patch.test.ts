// The AppImage environment Atlas owns (#266): which GDK backend the hook picks, that it no
// longer forces a GTK theme, and that the patch script strips host-ABI libraries (the bundled
// libwayland aborted every web process on Arch, #265) and refuses an AppDir that still has them.
import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const HOOK = join(root, "packaging/appimage/apprun-hook.sh");
const PATCH = join(root, "scripts/appimage-patch.sh");

const dirs: string[] = [];
afterEach(() => {
	for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function tmp() {
	const d = mkdtempSync(join(tmpdir(), "appimage-"));
	dirs.push(d);
	return d;
}

/** Source the hook in a clean env and print what it exported. */
function hookEnv(env: Record<string, string>): Record<string, string> {
	const d = tmp();
	const p = Bun.spawnSync(
		["bash", "-c", `source "${HOOK}"; printf 'GDK_BACKEND=%s\\nGTK_THEME=%s\\n' "$GDK_BACKEND" "\${GTK_THEME:-}"`],
		{ env: { PATH: "/usr/bin:/bin", APPDIR: d, ATLAS_NVIDIA_PROC: join(d, "no-nvidia"), ...env } },
	);
	expect(p.exitCode).toBe(0);
	return Object.fromEntries(
		p.stdout.toString().trim().split("\n").map((l) => l.split(/=(.*)/s).slice(0, 2) as [string, string]),
	);
}

// The AppImage, its AppRun hook and the patch script exist only on Linux; Windows runners have no bash.
describe.skipIf(process.platform === "win32")("apprun hook", () => {
	test("the user's GDK_BACKEND wins (Omarchy sets wayland,x11,*)", () => {
		expect(hookEnv({ GDK_BACKEND: "wayland,x11,*", WAYLAND_DISPLAY: "wayland-1" }).GDK_BACKEND).toBe("wayland,x11,*");
	});
	test("ATLAS_GDK_BACKEND overrides everything", () => {
		expect(hookEnv({ ATLAS_GDK_BACKEND: "x11", GDK_BACKEND: "wayland" }).GDK_BACKEND).toBe("x11");
	});
	test("nothing set: X11 even on a Wayland session, until #266's hardware matrix passes", () => {
		expect(hookEnv({ WAYLAND_DISPLAY: "wayland-0" }).GDK_BACKEND).toBe("x11");
		expect(hookEnv({ XDG_SESSION_TYPE: "wayland" }).GDK_BACKEND).toBe("x11");
	});
	test("NVIDIA's proprietary driver turns a session Wayland request into X11; ATLAS_GDK_BACKEND still forces it", () => {
		const d = tmp();
		writeFileSync(join(d, "version"), "NVRM version: 570");
		const nv = join(d, "version");
		expect(hookEnv({ GDK_BACKEND: "wayland,x11,*", ATLAS_NVIDIA_PROC: nv }).GDK_BACKEND).toBe("x11");
		expect(hookEnv({ ATLAS_GDK_BACKEND: "wayland", ATLAS_NVIDIA_PROC: nv }).GDK_BACKEND).toBe("wayland");
	});
	test("an X11 session stays on X11", () => {
		expect(hookEnv({ DISPLAY: ":0" }).GDK_BACKEND).toBe("x11");
	});
	test("no GTK theme is forced unless APPIMAGE_GTK_THEME asks", () => {
		expect(hookEnv({ WAYLAND_DISPLAY: "w" }).GTK_THEME).toBe("");
		expect(hookEnv({ APPIMAGE_GTK_THEME: "Adwaita:dark" }).GTK_THEME).toBe("Adwaita:dark");
	});
});

/** A stand-in for `readelf -d`: the libraries a fake plugin NEEDs are the lines of `<file>.needed` (the fakes are not ELF). */
function fakeReadelf(): string {
	const f = join(tmp(), "readelf");
	writeFileSync(
		f,
		'#!/bin/sh\n[ -f "$2.needed" ] || exit 0\nwhile read -r l; do echo " 0x0000000000000001 (NEEDED)             Shared library: [$l]"; done < "$2.needed"\n',
		{ mode: 0o755 },
	);
	return f;
}
function patch(appdir: string) {
	return Bun.spawnSync(["bash", PATCH, appdir], { env: { ...process.env, ATLAS_READELF: fakeReadelf() } });
}
function plugin(appdir: string, name: string, needs: string[]) {
	const f = join(appdir, "usr/lib/gstreamer-1.0", name);
	writeFileSync(f, "x");
	writeFileSync(`${f}.needed`, needs.join("\n") + "\n");
}

function fakeAppDir(): string {
	const a = join(tmp(), "AppDir");
	mkdirSync(join(a, "usr/lib"), { recursive: true });
	mkdirSync(join(a, "apprun-hooks"), { recursive: true });
	for (const f of ["libwayland-client.so.0", "libwayland-egl.so.1", "libwebkit2gtk-4.1.so.0", "libgbm.so.1"]) {
		writeFileSync(join(a, "usr/lib", f), "x");
	}
	writeFileSync(join(a, "apprun-hooks/linuxdeploy-plugin-gtk.sh"), "export GDK_BACKEND=x11\nexport GTK_THEME=Adwaita\n");
	// bundleMediaFramework: linuxdeploy's GStreamer plugin points GST_PLUGIN_PATH inside the AppImage.
	writeFileSync(join(a, "apprun-hooks/linuxdeploy-plugin-gstreamer.sh"), "export GST_PLUGIN_PATH=$APPDIR/usr/lib/gstreamer-1.0\n");
	writeFileSync(join(a, "usr/lib/libgstreamer-1.0.so.0"), "x");
	// The plugins linuxdeploy bundled: libgstapp.so is where `appsrc` lives.
	mkdirSync(join(a, "usr/lib/gstreamer-1.0"), { recursive: true });
	writeFileSync(join(a, "usr/lib/gstreamer-1.0/libgstapp.so"), "x");
	return a;
}

describe.skipIf(process.platform === "win32")("appimage-patch.sh", () => {
	test("strips host-ABI libraries, keeps the app's own, installs our hook", () => {
		const a = fakeAppDir();
		const p = patch(a);
		expect(p.exitCode).toBe(0);
		expect(readdirSync(join(a, "usr/lib")).sort()).toEqual(["gstreamer-1.0", "libgstreamer-1.0.so.0", "libwebkit2gtk-4.1.so.0"]);
		expect(readFileSync(join(a, "apprun-hooks/linuxdeploy-plugin-gtk.sh"), "utf8")).toBe(readFileSync(HOOK, "utf8"));
		// The bundled GStreamer and its plugins' hook stay (#265: no plugins → `appsrc not found`).
		expect(readFileSync(join(a, "apprun-hooks/linuxdeploy-plugin-gstreamer.sh"), "utf8")).toContain("GST_PLUGIN_PATH");
	});
	// #265: the 1.5.0-alpha.25 AppImage carried libgstreamer and no plugins, and every media element in the page logged
	// `GStreamer element appsrc not found` (and, with no plugin at all, aborted the web process). `appsrc` is in
	// libgstapp.so: a build whose bundle lacks it must not be published.
	test("refuses an AppDir whose GStreamer has no plugins (appsrc not found)", () => {
		const a = fakeAppDir();
		rmSync(join(a, "usr/lib/gstreamer-1.0"), { recursive: true });
		const p = patch(a);
		expect(p.exitCode).not.toBe(0);
		expect(p.stderr.toString()).toContain("libgstapp.so");
	});
	test("refuses an AppDir with plugins but without linuxdeploy's GStreamer hook (they would not be found off Ubuntu)", () => {
		const a = fakeAppDir();
		rmSync(join(a, "apprun-hooks/linuxdeploy-plugin-gstreamer.sh"));
		const p = patch(a);
		expect(p.exitCode).not.toBe(0);
		expect(p.stderr.toString()).toContain("linuxdeploy-plugin-gstreamer.sh");
	});
	// The failed build-only release: libgstjack.so needs libjack.so.0, which linuxdeploy never bundles, so
	// gst-plugin-scanner warned `Failed to load plugin ... libjack.so.0: cannot open shared object file` and the launch
	// check (which reads the app's stderr) failed on Ubuntu 24.04 and Arch. A plugin must not ship a dependency nobody ships.
	test("drops a GStreamer plugin whose library is neither bundled nor a host library, keeps the others", () => {
		const a = fakeAppDir();
		writeFileSync(join(a, "usr/lib/libraw1394.so.11"), "x");
		plugin(a, "libgstapp.so", ["libgstreamer-1.0.so.0", "libc.so.6", "libm.so.6"]);
		plugin(a, "libgstjack.so", ["libgstreamer-1.0.so.0", "libjack.so.0", "libc.so.6"]);
		plugin(a, "libgst1394.so", ["libraw1394.so.11", "libc.so.6"]);
		plugin(a, "libgsttaglib.so", ["libstdc++.so.6", "libgcc_s.so.1", "libz.so.1"]);
		plugin(a, "libgstximagesrc.so", ["libX11.so.6"]);
		plugin(a, "libgstopengl.so", ["libEGL.so.1", "libGL.so.1", "libgbm.so.1"]);
		const p = patch(a);
		expect(p.exitCode).toBe(0);
		expect(p.stderr.toString()).toContain("dropping GStreamer plugin libgstjack.so: needs libjack.so.0");
		const kept = readdirSync(join(a, "usr/lib/gstreamer-1.0")).filter((f) => f.endsWith(".so")).sort();
		expect(kept).toEqual(["libgst1394.so", "libgstapp.so", "libgstopengl.so", "libgsttaglib.so", "libgstximagesrc.so"]);
	});
	test("a plugin whose only missing library is the one that makes media work still fails the build, never silently", () => {
		const a = fakeAppDir();
		plugin(a, "libgstapp.so", ["libgstreamer-1.0.so.0", "libappsrc-private.so.1"]);
		const p = patch(a);
		expect(p.exitCode).not.toBe(0);
		expect(p.stderr.toString()).toContain("libgstapp.so");
	});
	test("an unreadable plugin fails the build instead of being kept unchecked", () => {
		const a = fakeAppDir();
		const bad = join(tmp(), "readelf");
		writeFileSync(bad, "#!/bin/sh\necho 'not an ELF' >&2\nexit 1\n", { mode: 0o755 });
		const p = Bun.spawnSync(["bash", PATCH, a], { env: { ...process.env, ATLAS_READELF: bad } });
		expect(p.exitCode).not.toBe(0);
		expect(p.stderr.toString()).toContain("cannot read the libraries of");
	});
	test("refuses a dir that is not an AppDir", () => {
		const d = tmp();
		const p = patch(d);
		expect(p.exitCode).not.toBe(0);
		expect(existsSync(join(d, "apprun-hooks"))).toBe(false);
	});
});
