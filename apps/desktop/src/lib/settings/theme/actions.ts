/** Export, remove and reveal actions for themes, shared by the Appearance tab. */
import { toast } from "@codeatlasdev/ui";
import { homeDir } from "@tauri-apps/api/path";
import { api, errText } from "#lib/api.ts";
import { host } from "#lib/host.ts";
import { m } from "#lib/i18n/m.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { themes } from "#lib/stores/theme.svelte.ts";

export type ExportFormat = "json" | "ghostty";

const EXT: Record<ExportFormat, string> = { json: "json", ghostty: "ghostty" };
const slug = (name: string) =>
	name
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-|-$/g, "") || "theme";

/** Run `fn`; failures become an error toast instead of an unhandled rejection. */
async function guard(fn: () => Promise<void>) {
	try {
		await fn();
	} catch (e) {
		toast.error(errText(e));
	}
}

export const copyTheme = (name: string, format: ExportFormat) =>
	guard(async () => {
		await navigator.clipboard.writeText(themes.exportTheme(name, format));
		toast.success(
			format === "json"
				? m.settings_theme_actions_copied_json({ name })
				: m.settings_theme_actions_copied_ghostty({ name }),
		);
	});

export const saveTheme = (name: string, format: ExportFormat) =>
	guard(async () => {
		const text = themes.exportTheme(name, format);
		const path = await api.savePath({
			title: m.settings_theme_actions_save_title({ name }),
			defaultPath: `${await homeDir()}/Downloads/${slug(name)}.${EXT[format]}`,
			filters: [
				{
					name:
						format === "json"
							? m.settings_theme_actions_filter_atlas()
							: m.settings_theme_actions_filter_ghostty(),
					extensions: [EXT[format]],
				},
			],
		});
		if (!path) return;
		await host.fsWrite(path, text);
		toast.success(m.settings_theme_actions_saved({ file: path.split("/").pop() ?? path }));
	});

export const removeTheme = (name: string) =>
	guard(async () => {
		const ok = await confirmDialog(m.settings_theme_actions_remove_title({ name }), {
			description: m.settings_theme_actions_remove_description(),
			confirmLabel: m.common_action_delete(),
			destructive: true,
		});
		if (ok) await themes.remove(name);
	});

/** Open `~/.atlas/themes` in Finder (created on first use). */
export const revealThemesFolder = () =>
	guard(async () => {
		const dir = `${await homeDir()}/.atlas/themes`;
		await host.fsMkdir(dir).catch(() => {});
		await host.exec(dir, ["open", dir]);
	});
