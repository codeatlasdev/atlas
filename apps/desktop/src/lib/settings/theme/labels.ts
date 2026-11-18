import { m } from "#lib/i18n/m.ts";
import type { ThemeFormat } from "#lib/theme/import/detect.ts";

/**
 * Words the theme screens draw from data, never from a typed string. Theme names (Atlas Dark, Dracula, One Dark, the
 * names of a user's own themes) are product or file names and stay as they are; what is translated is the descriptive
 * vocabulary around them: dark and light, the accent colors, the names of the formats a theme can be imported from.
 */

/** `dark` / `light` as a person reads it. */
export const appearanceLabel = (appearance: "dark" | "light") =>
	appearance === "dark" ? m.settings_theme_mode_dark() : m.settings_theme_mode_light();

/**
 * The accent colors of the kit (`THEME_PRESETS` in `@codeatlasdev/ui`), by preset id: the kit names them in English only.
 * `labels.test.ts` reads the kit's list, so a color added in an update fails there until it has a message.
 */
export const ACCENT_MESSAGES: Record<string, () => string> = {
	amber: () => m.settings_theme_accent_amber(),
	ocean: () => m.settings_theme_accent_ocean(),
	crimson: () => m.settings_theme_accent_crimson(),
	cyan: () => m.settings_theme_accent_cyan(),
	violet: () => m.settings_theme_accent_violet(),
	emerald: () => m.settings_theme_accent_emerald(),
	rose: () => m.settings_theme_accent_rose(),
	slate: () => m.settings_theme_accent_slate(),
};

/** An accent color's name; a preset this build has no message for shows the kit's own label. */
export function accentLabel(preset: { id: string; label: string }): string {
	return Object.hasOwn(ACCENT_MESSAGES, preset.id) ? ACCENT_MESSAGES[preset.id]!() : preset.label;
}

/** What the import dialog calls the format it detected (a descriptive name: "Ghostty config", not just "Ghostty"). */
export const FORMAT_MESSAGES: Record<ThemeFormat, () => string> = {
	ghostty: () => m.settings_theme_import_format_ghostty(),
	iterm: () => m.settings_theme_import_format_iterm(),
	vscode: () => m.settings_theme_import_format_vscode(),
	base16: () => m.settings_theme_import_format_base16(),
	zed: () => m.settings_theme_import_format_zed(),
	atlas: () => m.settings_theme_import_format_atlas(),
};
