// "Open any file with Atlas" (#499) is wired in four places that nothing compiles against: Tauri's `fileAssociations`
// (macOS Info.plist), the Linux .desktop template, and the Windows NSIS hooks. These tests pin the contract the
// feature stands on: Atlas is offered, and it is never the default.
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (f: string) => readFileSync(join(root, f), "utf8");
const conf = (f: string) => JSON.parse(read(`crates/atlas-desktop/${f}`));

describe("macOS: CFBundleDocumentTypes from bundle.fileAssociations", () => {
	const assoc: any[] = conf("tauri.conf.json").bundle.fileAssociations;
	const types = assoc.flatMap((a) => a.contentTypes ?? []);

	test("every declared type has the Alternate rank, so Atlas is offered in Open With and never becomes the default", () => {
		expect(assoc.length).toBeGreaterThan(0);
		for (const a of assoc) expect(a.rank, a.name).toBe("Alternate");
	});

	test("text, source, data and folders are all declared", () => {
		for (const t of [
			"public.text",
			"public.plain-text",
			"public.source-code",
			"public.script",
			"public.shell-script",
			"public.json",
			"public.xml",
			"public.yaml",
			"net.daringfireball.markdown",
			"public.data",
			"public.folder",
		])
			expect(types, t).toContain(t);
		expect(assoc.flatMap((a) => a.ext)).toEqual(expect.arrayContaining(["md", "markdown", "txt"]));
	});

	test("a folder is a viewer role and every association is named (Launch Services shows the name)", () => {
		const folder = assoc.find((a) => a.contentTypes?.includes("public.folder"));
		expect(folder.role).toBe("Viewer");
		for (const a of assoc) expect(a.name, JSON.stringify(a)).toBeTruthy();
	});
});

describe("Windows: NSIS 'Open with Atlas', per user, never the default", () => {
	const nsh = read("packaging/windows/installer-hooks.nsh");
	const lines = nsh.split("\n");
	const classes = lines.filter((l) => /WriteReg\w+|DeleteReg\w+/.test(l) && l.includes("Software\\Classes"));

	test("Tauri's own fileAssociations stay empty on Windows (its NSIS template would write the default handler)", () => {
		expect(conf("tauri.windows.conf.json").bundle.fileAssociations).toEqual([]);
	});

	test("every association key is under HKCU, never HKLM, HKCR or SHCTX", () => {
		expect(classes.length).toBeGreaterThan(10);
		for (const l of classes) expect(l.trim(), l).toMatch(/^(WriteRegStr|DeleteRegKey|DeleteRegValue) HKCU /);
		const association = nsh.split("ATLAS_EACH_EXT M")[0] + nsh.split("!macro NSIS_HOOK_POSTINSTALL")[0].split("ATLAS_EACH_EXT M")[1];
		expect(association).not.toMatch(/HKLM|HKCR|SHCTX/);
	});

	test("an extension only gets an OpenWithProgids value: no default value is ever written on an extension key", () => {
		for (const l of classes.filter((x) => x.includes("Classes\\${EXT}"))) {
			expect(l, l).toContain("\\OpenWithProgids");
		}
		expect(nsh).not.toMatch(/WriteRegStr HKCU "Software\\Classes\\\.[^"]*" "" /);
		expect(nsh).not.toMatch(/UserChoice|FileExts/);
	});

	test("the uninstaller removes every key and value the installer writes", () => {
		const keys = (op: string) => [...nsh.matchAll(new RegExp(`${op} HKCU "([^"]+)"`, "g"))].map((m) => m[1]);
		const written = new Set(
			keys("WriteRegStr").map((k) =>
				k
					.replace(/\\SupportedTypes$/, "")
					.replace(/\\shell\\open\\command$/, "")
					.replace(/\\DefaultIcon$/, "")
					.replace(/\\command$/, ""),
			),
		);
		const removedKeys = new Set(keys("DeleteRegKey"));
		for (const k of written) {
			if (k.includes("${EXT}")) continue; // values under extension keys: removed one by one below
			expect(removedKeys, k).toContain(k);
		}
		expect(nsh).toContain('DeleteRegValue HKCU "Software\\Classes\\${EXT}\\OpenWithProgids" "AtlasDesktop.File"');
	});

	test("install and uninstall run from the one extension list, from the install and uninstall hooks", () => {
		expect(nsh).toContain("!insertmacro ATLAS_EACH_EXT ATLAS_EXT_REGISTER");
		expect(nsh).toContain("!insertmacro ATLAS_EACH_EXT ATLAS_EXT_UNREGISTER");
		const post = nsh.split("!macro NSIS_HOOK_POSTINSTALL")[1].split("!macroend")[0];
		const un = nsh.split("!macro NSIS_HOOK_POSTUNINSTALL")[1].split("!macroend")[0];
		expect(post).toContain("ATLAS_OPENWITH_REGISTER");
		expect(un).toContain("ATLAS_OPENWITH_UNREGISTER");
		const list = nsh.split("!macro ATLAS_EACH_EXT M")[1].split("!macroend")[0];
		for (const e of [".md", ".txt", ".json", ".rs", ".ts", ".py", ".sh", ".yaml"]) expect(list, e).toContain(`"${e}"`);
	});

	test("files and folders open through the exe with the path, and the folder background gets the current folder", () => {
		expect(nsh).toContain('Directory\\shell\\AtlasDesktop\\command" "" "$\\"$INSTDIR\\${MAINBINARYNAME}.exe$\\" $\\"%1$\\""');
		expect(nsh).toContain('Directory\\Background\\shell\\AtlasDesktop\\command" "" "$\\"$INSTDIR\\${MAINBINARYNAME}.exe$\\" $\\"%V$\\""');
	});
});

describe("Linux: the .desktop file of the deb, rpm, AppImage and Arch package", () => {
	const linux = conf("tauri.linux.conf.json").bundle;
	const tpl = read("packaging/linux/atlas.desktop");
	const field = (k: string) => tpl.split("\n").find((l) => l.startsWith(`${k}=`)) ?? "";
	const mimes = field("MimeType").slice("MimeType=".length).split(";").filter(Boolean);

	test("deb and rpm use the one template, which exists (the AppImage and the Arch package are built from the deb)", () => {
		for (const k of ["deb", "rpm"]) {
			const rel = linux.linux[k].desktopTemplate;
			expect(rel, k).toBe("../../packaging/linux/atlas.desktop");
			expect(existsSync(join(root, "crates/atlas-desktop", rel)), k).toBe(true);
		}
		expect(linux.fileAssociations).toEqual([]);
		expect(read("packaging/arch/PKGBUILD.in")).toContain("bsdtar -xf data.tar");
	});

	test("it keeps every field Tauri fills in and takes files from the file manager (%F, several at once)", () => {
		for (const v of ["{{categories}}", "{{icon}}", "{{name}}"]) expect(tpl).toContain(v);
		expect(field("Exec")).toBe("Exec={{exec}} %F");
		expect(field("Type")).toBe("Type=Application");
		expect(field("Terminal")).toBe("Terminal=false");
	});

	test("it lists text, Markdown, source, data and folders, and nothing that would make Atlas the default for media", () => {
		for (const m of ["text/plain", "text/markdown", "text/x-python", "text/x-rust", "application/json", "application/x-yaml", "inode/directory"])
			expect(mimes, m).toContain(m);
		expect(mimes.filter((m) => /^(image|audio|video)\//.test(m))).toEqual([]);
		expect(new Set(mimes).size).toBe(mimes.length);
	});
});

describe("single instance: a second Open With goes to the running window", () => {
	test("Windows and Linux hand the second launch's argv and cwd to the open queue; macOS gets the open event", () => {
		const main = read("crates/atlas-desktop/src/main.rs");
		const guard = main.split("tauri_plugin_single_instance::init")[1].split("show_main(app)")[0];
		expect(guard).toContain("open_path::paths_from_argv(&argv");
		expect(guard).toContain("open_path::open(");
		expect(main).toContain("tauri::RunEvent::Opened { urls }");
	});
});
