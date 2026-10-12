import { homeDir } from "@tauri-apps/api/path";
import { colorScheme } from "@codeatlasdev/ui";
import { api } from "#lib/api.ts";
import { host, b64decode } from "#lib/host.ts";
import {
	cssVars,
	parseSelection,
	parseThemeFamily,
	resolveTheme,
	terminalColors,
	xtermTheme,
	type Theme,
	type ThemeSelection,
} from "#lib/theme.ts";

const bundled = import.meta.glob<unknown>("../themes/*.json", { eager: true, import: "default" });

class Themes {
	/** Bundled first, then `~/.atlas/desktop/themes/*.json` (a user theme with the same name wins). */
	list = $state<Theme[]>(Object.values(bundled).flatMap((f) => parseThemeFamily(f)));
	selection = $state<ThemeSelection>(parseSelection(undefined));
	/** Bumped when the OS appearance may have changed, so `current` re-resolves in system mode. */
	private systemTick = $state(0);

	current = $derived.by(() => {
		void this.systemTick;
		return resolveTheme(this.list, this.selection, matchMedia("(prefers-color-scheme: dark)").matches);
	});

	names = $derived({
		light: this.list.filter((t) => t.appearance === "light").map((t) => t.name),
		dark: this.list.filter((t) => t.appearance === "dark").map((t) => t.name),
	});

	/** Terminal colors of the active theme; `null` fields mean "use the kit surface/text vars". */
	terminal = $derived(this.current ? terminalColors(this.current.style) : null);
	xterm = $derived(this.current ? xtermTheme(this.current.style) : {});

	private applied: string[] = [];

	async init() {
		this.selection = parseSelection((await api.settings()).theme);
		await this.loadUserThemes();
		this.apply();
		matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
			this.systemTick++;
			this.apply();
		});
	}

	/** Bad files are skipped: a broken user theme must not take the app down or hide the others. */
	private async loadUserThemes() {
		const dir = `${await homeDir()}/.atlas/desktop/themes`;
		const entries = await host.fsList(dir).catch(() => []);
		const user: Theme[] = [];
		for (const e of entries) {
			if (e.kind !== "file" || !e.name.endsWith(".json")) continue;
			try {
				const r = await host.fsRead(`${dir}/${e.name}`, 2_000_000);
				user.push(...parseThemeFamily(JSON.parse(b64decode(r.data))));
			} catch {
				// invalid JSON or unreadable file
			}
		}
		const names = new Set(user.map((t) => t.name));
		this.list = [...this.list.filter((t) => !names.has(t.name)), ...user];
	}

	async setTheme(patch: Partial<ThemeSelection>) {
		this.selection = { ...this.selection, ...patch };
		this.apply();
		await api.settingsPatch({ theme: this.selection });
	}

	/** Writes the active theme onto <html> and aligns the kit scheme (native sidebar material follows it). */
	private apply() {
		const root = document.documentElement;
		const t = this.current;
		for (const v of this.applied) root.style.removeProperty(v);
		this.applied = [];
		if (!t) return;
		// The kit owns the `.dark` class and the layout pins the native window theme to its preference.
		if (colorScheme.preference !== this.selection.mode) colorScheme.setPreference(this.selection.mode);
		for (const [k, v] of Object.entries(cssVars(t.style))) {
			root.style.setProperty(k, v);
			this.applied.push(k);
		}
	}
}

export const themes = new Themes();
