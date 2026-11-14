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

describe("apprun hook", () => {
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

function fakeAppDir(): string {
	const a = join(tmp(), "AppDir");
	mkdirSync(join(a, "usr/lib"), { recursive: true });
	mkdirSync(join(a, "apprun-hooks"), { recursive: true });
	for (const f of ["libwayland-client.so.0", "libwayland-egl.so.1", "libwebkit2gtk-4.1.so.0", "libgbm.so.1"]) {
		writeFileSync(join(a, "usr/lib", f), "x");
	}
	writeFileSync(join(a, "apprun-hooks/linuxdeploy-plugin-gtk.sh"), "export GDK_BACKEND=x11\nexport GTK_THEME=Adwaita\n");
	return a;
}

describe("appimage-patch.sh", () => {
	test("strips host-ABI libraries, keeps the app's own, installs our hook", () => {
		const a = fakeAppDir();
		const p = Bun.spawnSync(["bash", PATCH, a]);
		expect(p.exitCode).toBe(0);
		expect(readdirSync(join(a, "usr/lib")).sort()).toEqual(["libwebkit2gtk-4.1.so.0"]);
		expect(readFileSync(join(a, "apprun-hooks/linuxdeploy-plugin-gtk.sh"), "utf8")).toBe(readFileSync(HOOK, "utf8"));
	});
	test("refuses a dir that is not an AppDir", () => {
		const d = tmp();
		const p = Bun.spawnSync(["bash", PATCH, d]);
		expect(p.exitCode).not.toBe(0);
		expect(existsSync(join(d, "apprun-hooks"))).toBe(false);
	});
});
