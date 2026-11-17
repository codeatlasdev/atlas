// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { compile } from "@inlang/paraglide-js";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { kbd, revealLabel } from "#lib/keys.ts";
import { appLocale } from "./locale.svelte.ts";
import { m, withOs } from "./m.ts";
import { inLocale } from "../../../test/locale.ts";

const OSES = ["mac", "windows", "linux"] as const;
const LOCALES = ["en", "pt-BR"] as const;
const select = (mac: string, win: string, other: string) => [
	{
		declarations: ["input os"],
		selectors: ["os"],
		match: { "os=mac": mac, "os=windows": win, "os=*": other },
	},
];

// A scratch catalog with the two shapes of OS-dependent text (an OS noun picked by `os`, a shortcut passed as a
// placeholder), compiled the way the app compiles its own, so the wrapper is tested on what Paraglide really generates.
async function scratch() {
	const dir = mkdtempSync(join(tmpdir(), "atlas-i18n-"));
	mkdirSync(join(dir, "project.inlang"));
	mkdirSync(join(dir, "messages"));
	const plugin = join(import.meta.dir, "../../../node_modules/@inlang/plugin-message-format/dist/index.js");
	writeFileSync(
		join(dir, "project.inlang/settings.json"),
		JSON.stringify({
			baseLocale: "en",
			locales: ["en", "pt-BR"],
			modules: [plugin],
			"plugin.inlang.messageFormat": { pathPattern: "./messages/{locale}.json" },
		}),
	);
	const catalogs = {
		en: {
			machine: select("your Mac", "this PC", "this computer"),
			trash: select("Move to Trash", "Move to Recycle Bin", "Move to Trash"),
			save: "Press {key} to save",
		},
		"pt-BR": {
			machine: select("seu Mac", "este PC", "este computador"),
			trash: select("Mover para a Lixeira", "Mover para a Lixeira", "Mover para a Lixeira"),
			save: "Pressione {key} para salvar",
		},
	};
	for (const [l, c] of Object.entries(catalogs))
		writeFileSync(join(dir, "messages", `${l}.json`), JSON.stringify(c));
	await compile({
		project: join(dir, "project.inlang"),
		outdir: join(dir, "out"),
		strategy: ["custom-atlas", "baseLocale"],
	});
	const runtime = await import(join(dir, "out/runtime.js"));
	// The scratch runtime reads the app's language like the real one does.
	runtime.defineCustomClientStrategy("custom-atlas", { getLocale: () => appLocale(), setLocale: () => {} });
	return (await import(join(dir, "out/messages.js"))).m;
}

test("an OS noun is chosen by the injected os, one message x three systems x two languages", async () => {
	const generated = await scratch();
	const table = {};
	for (const locale of LOCALES)
		await inLocale(locale, () => {
			for (const os of OSES)
				table[`${locale}/${os}`] = [
					withOs(generated, () => os)
						.machine()
						.toString(),
					withOs(generated, () => os)
						.trash()
						.toString(),
				];
		});
	expect(table).toEqual({
		"en/mac": ["your Mac", "Move to Trash"],
		"en/windows": ["this PC", "Move to Recycle Bin"],
		"en/linux": ["this computer", "Move to Trash"],
		"pt-BR/mac": ["seu Mac", "Mover para a Lixeira"],
		"pt-BR/windows": ["este PC", "Mover para a Lixeira"],
		"pt-BR/linux": ["este computador", "Mover para a Lixeira"],
	});
});

test("a call site never passes os, but may say another one", async () => {
	const wrapped = withOs(await scratch(), () => "windows");
	expect(wrapped.machine()).toBe("this PC");
	expect(wrapped.machine({ os: "mac" })).toBe("your Mac");
});

test("a shortcut is a placeholder the caller fills from keys.ts: the text differs between systems only by that value", async () => {
	const generated = await scratch();
	for (const locale of LOCALES)
		await inLocale(locale, () => {
			const [mac, other] = [kbd("cmd-shift-p", true), kbd("cmd-shift-p", false)];
			expect([mac, other]).toEqual(["⇧⌘P", "Ctrl+Shift+P"]);
			const on = (os, key) =>
				withOs(generated, () => os)
					.save({ key })
					.toString();
			expect(on("mac", mac).replace(mac, "{key}")).toBe(on("windows", other).replace(other, "{key}"));
			expect(on("mac", mac)).not.toBe(on("windows", other));
		});
});

test("the app's own message goes through the same wrapper: the file manager of each system, in each language", async () => {
	const table = {};
	for (const locale of LOCALES)
		await inLocale(locale, () => {
			for (const os of OSES) table[`${locale}/${os}`] = revealLabel(os);
		});
	expect(table).toEqual({
		"en/mac": "Reveal in Finder",
		"en/windows": "Reveal in File Explorer",
		"en/linux": "Reveal in file manager",
		"pt-BR/mac": "Mostrar no Finder",
		"pt-BR/windows": "Mostrar no Explorador de Arquivos",
		"pt-BR/linux": "Mostrar no gerenciador de arquivos",
	});
	expect(m.os_reveal_in_file_manager()).toBe("Reveal in Finder");
});
