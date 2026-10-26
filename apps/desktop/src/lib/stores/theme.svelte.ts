import { invoke } from "@tauri-apps/api/core";
import { bootMark } from "#lib/boot.ts";
import { homeDir } from "@tauri-apps/api/path";
import { listen } from "@tauri-apps/api/event";
import { colorScheme } from "@codeatlasdev/ui";
import { api } from "#lib/api.ts";
import { host, b64decode } from "#lib/host.ts";
import {
	cssVars,
	migrateSelection,
	parseFamily,
	resolveTerminalTheme,
	resolveTheme,
	terminalColors,
	xtermTheme,
	type Issue,
	type Theme,
	type ThemeSelection,
} from "#lib/theme.ts";
import { writeThemeCache } from "#lib/theme/cache.ts";
import { complete } from "#lib/theme/derive.ts";
import { exportAtlasJson, exportGhostty } from "#lib/theme/import/export.ts";
import { parseGhosttyConfig } from "#lib/theme/import/ghostty.ts";
import type { ImportResult } from "#lib/theme/import/color.ts";
import { allVars } from "#lib/theme/tokens.ts";
import { prefs, setPref } from "#lib/stores/prefs.svelte.ts";

const bundled = import.meta.glob<unknown>("../themes/*.json", { eager: true, import: "default" });

const WATCH_ID = "atlas-themes";
const slug = (name: string) =>
	name
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-|-$/g, "") || "theme";

/** Parse the bundled families once; they are trusted, so issues only matter in tests. */
function bundledThemes(): Theme[] {
	return Object.values(bundled).flatMap((f) =>
		parseFamily(f).themes.map((t) => ({ ...t, source: "bundled" as const })),
	);
}

/**
 * A user theme file as `themes_user_read` returns it: its text, or why it could not be read.
 * Read locally in Rust, so theme loading never waits for the terminal host to connect.
 */
interface UserFile {
	file: string;
	text?: string;
	error?: string;
}

interface FileIssues {
	file: string;
	issues: Issue[];
}

class Themes {
	/** Bundled first, then user themes (a user theme with the same name wins). Raw: not yet completed. */
	list = $state<Theme[]>(bundledThemes());
	selection = $state<ThemeSelection>(migrateSelection(undefined).selection);
	/** `true` once the user chose something (or a legacy non-default value was found). */
	explicit = $state(false);
	issues = $state<FileIssues[]>([]);
	/** The user's selection and theme files are in: `current` is the real theme, not a fallback. */
	ready = $state(false);
	ghostty = $state({ available: false, offered: false });
	/** Bumped when the OS appearance may have changed, so `current` re-resolves in system mode. */
	private systemTick = $state(0);
	private applied: string[] = [];
	private home = "";

	private resolve(name: string | null) {
		void this.systemTick;
		const dark = matchMedia("(prefers-color-scheme: dark)").matches;
		const t = name ? this.list.find((x) => x.name === name) : resolveTheme(this.list, this.selection, dark);
		return t && { ...t, style: complete(t.style, t.appearance) };
	}

	/** The active theme with every token defined (derived where the file left gaps). */
	current = $derived.by(() => this.resolve(null));

	/** The user's dark theme, completed: surfaces that are always dark (the welcome sky) paint their text with it. */
	currentDark = $derived.by(() => {
		const t = resolveTheme(this.list, { ...this.selection, mode: "dark" }, true);
		return t && { ...t, style: complete(t.style, t.appearance) };
	});

	currentTerminal = $derived.by(() => {
		const t = resolveTerminalTheme(this.list, this.selection.terminal, this.current);
		return t && t === this.current ? t : t && { ...t, style: complete(t.style, t.appearance) };
	});

	names = $derived({
		light: this.list.filter((t) => t.appearance === "light").map((t) => t.name),
		dark: this.list.filter((t) => t.appearance === "dark").map((t) => t.name),
	});

	/** Terminal colors; `null` fields mean "use the kit surface/text vars". */
	terminal = $derived(this.currentTerminal ? terminalColors(this.currentTerminal.style) : null);
	xterm = $derived(this.currentTerminal ? xtermTheme(this.currentTerminal.style) : {});

	/**
	 * Settings and user theme files are read in parallel straight from disk (no terminal-host
	 * round trip), so the real theme resolves in one IPC hop; hostd-dependent work (watching the
	 * theme dir, the Ghostty probe) follows once the theme is applied.
	 */
	async init() {
		bootMark("theme:init");
		const [settings, files, home] = await Promise.all([
			api.settings(),
			invoke<UserFile[]>("themes_user_read"),
			homeDir(),
		]);
		this.home = home;
		const m = migrateSelection(settings.theme);
		this.selection = m.selection;
		this.explicit = m.explicit;
		this.load(files);
		this.ready = true;
		bootMark("theme:ready");
		matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
			this.systemTick++;
			this.apply();
		});
		await this.watch();
		await this.probeGhostty(settings.ghosttyThemeOffer);
	}

	/** `~/.atlas/themes` is the user dir; the pre-1.2 `~/.atlas/desktop/themes` is still read. */
	private get userDir() {
		return `${this.home}/.atlas/themes`;
	}

	/** Re-read every user theme file. Bad files are reported in `issues` and skipped. */
	async reload() {
		this.load(await invoke<UserFile[]>("themes_user_read"));
	}

	private load(files: UserFile[]) {
		const user: Theme[] = [];
		const issues: FileIssues[] = [];
		for (const { file, text, error } of files) {
			try {
				if (error !== undefined) throw new Error(error);
				const parsed = parseFamily(JSON.parse(text ?? ""));
				if (parsed.issues.length) issues.push({ file, issues: parsed.issues });
				user.push(...parsed.themes.map((t) => ({ ...t, source: "user" as const, file })));
			} catch (err) {
				issues.push({
					file,
					issues: [{ path: "$", level: "error", message: `not valid JSON: ${(err as Error).message}` }],
				});
			}
		}
		const names = new Set(user.map((t) => t.name));
		this.list = [...bundledThemes().filter((t) => !names.has(t.name)), ...user];
		this.issues = issues;
		this.apply();
	}

	/** Hot reload: any change under the user dir re-reads it (debounced; saves arrive as several events). */
	private async watch() {
		await host.fsMkdir(this.userDir).catch(() => {});
		await host.fsWatch(this.userDir, WATCH_ID).catch(() => {});
		let timer: number | undefined;
		await listen<{ ev: string; id?: string }>("host://event", (e) => {
			if (e.payload.ev !== "fs_event" || e.payload.id !== WATCH_ID) return;
			clearTimeout(timer);
			timer = window.setTimeout(() => void this.reload(), 150);
		});
	}

	async setTheme(patch: Partial<Omit<ThemeSelection, "v">>) {
		this.selection = { ...this.selection, ...patch };
		this.explicit = true;
		this.apply();
		await api.settingsPatch({ theme: this.selection });
	}

	/** Next/previous theme of the active appearance, wrapping. */
	cycle(dir: 1 | -1) {
		const t = this.current;
		if (!t) return;
		const same = this.list.filter((x) => x.appearance === t.appearance);
		const i = same.findIndex((x) => x.name === t.name);
		const next = same[(i + dir + same.length) % same.length];
		if (next) void this.setTheme({ [t.appearance]: next.name });
	}

	// --- import / export -------------------------------------------------

	/** Save an imported theme as `~/.atlas/themes/<slug>.json`; optionally select it and adopt its font. */
	async importResult(r: ImportResult, opts: { applyFont?: boolean; select?: boolean } = {}): Promise<Theme> {
		const text = exportAtlasJson({ name: r.name, appearance: r.appearance, style: r.style, font: r.font });
		await host.fsMkdir(this.userDir).catch(() => {});
		await host.fsWrite(`${this.userDir}/${slug(r.name)}.json`, text);
		await this.reload();
		const theme = this.list.find((t) => t.name === r.name);
		if (!theme) throw new Error(`Imported "${r.name}" but it did not load; see the theme issues list.`);
		if (opts.applyFont) await this.applyImportedPrefs(r);
		if (opts.select) await this.setTheme({ [r.appearance]: r.name, mode: r.appearance, terminal: "app" });
		return theme;
	}

	/** Font and terminal prefs that came with an import (Ghostty config): written to `terminal.*`. */
	private async applyImportedPrefs(r: ImportResult) {
		const t: Record<string, unknown> = {};
		if (r.font?.family) t.font_family = r.font.family;
		if (r.font?.size) t.font_size = r.font.size;
		if (r.font?.thicken !== undefined) t.font_thicken = r.font.thicken;
		Object.assign(t, r.terminal ?? {});
		if (Object.keys(t).length) await setPref("terminal", { ...prefs.value.terminal, ...t });
	}

	exportTheme(name: string, format: "json" | "ghostty"): string {
		const t = this.resolve(name);
		if (!t) throw new Error(`No theme named "${name}"`);
		const p = prefs.value.terminal;
		return format === "json"
			? exportAtlasJson(t)
			: exportGhostty(t, {
					cursor_shape: p.cursor_shape,
					blinking: p.blinking,
					padding_x: p.padding_x,
					padding_y: p.padding_y,
				});
	}

	async remove(name: string) {
		const t = this.list.find((x) => x.name === name);
		if (!t?.file || t.source !== "user") return;
		const others = this.list.filter((x) => x.file === t.file && x.name !== name);
		if (others.length) throw new Error(`${t.file} holds other themes too; edit the file instead.`);
		await host.fsRemove(t.file);
		await this.reload();
	}

	// --- Ghostty ----------------------------------------------------------

	private get ghosttyConfig() {
		return `${this.home}/.config/ghostty/config`;
	}

	private async probeGhostty(offer: unknown) {
		const available = await host.fsStat(this.ghosttyConfig).then(
			(s) => s.kind === "file",
			() => false,
		);
		const dismissed = !!offer;
		this.ghostty = { available, offered: available && !dismissed };
	}

	async dismissGhosttyOffer() {
		this.ghostty = { ...this.ghostty, offered: false };
		await api.settingsPatch({ ghosttyThemeOffer: "dismissed" });
	}

	/** Parse `~/.config/ghostty/config` (includes and `theme =` resolved from the usual dirs). */
	async importGhostty(): Promise<ImportResult> {
		const read = async (path: string) => {
			try {
				return b64decode((await host.fsRead(path, 2_000_000)).data);
			} catch {
				return null;
			}
		};
		const text = await read(this.ghosttyConfig);
		if (text === null) throw new Error("No Ghostty config at ~/.config/ghostty/config");
		return parseGhosttyConfig(text, {
			home: this.home,
			readFile: read,
			themeDirs: [
				`${this.home}/.config/ghostty/themes`,
				"/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
			],
			appearance: this.current?.appearance ?? "dark",
		});
	}

	// --- apply ------------------------------------------------------------

	/** Writes the active theme onto <html> and aligns the kit scheme (native sidebar material follows it). */
	private apply() {
		const root = document.documentElement;
		const t = this.current;
		for (const v of [...this.applied, ...allVars()]) root.style.removeProperty(v);
		this.applied = [];
		if (!t) return;
		// The kit owns the `.dark` class and the layout pins the native window theme to its preference.
		if (colorScheme.preference !== this.selection.mode) colorScheme.setPreference(this.selection.mode);
		const vars = cssVars(t.style);
		for (const [k, v] of Object.entries(vars)) {
			root.style.setProperty(k, v);
			this.applied.push(k);
		}
		root.dataset.atlasTheme = t.name;
		writeThemeCache({
			name: t.name,
			scheme: this.selection.mode,
			dark: t.appearance === "dark",
			vars,
		});
	}
}

export const themes = new Themes();
