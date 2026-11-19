/**
 * Settings navigation with its text: the sidebar sections and the tab strips, from the registry. Where an id opens
 * (`resolveSection`, `openSection`) is `targets.ts`, which has no text and loads at startup; this module loads with
 * the Settings view. Pure: no Svelte, no IPC.
 */
import { IS_MAC } from "#lib/keys.ts";
import { REGISTRY, forPlatform } from "./registry.ts";
import type { SettingsSection, SettingsTab } from "./targets.ts";

export { openSection, resolveSection } from "./targets.ts";
export type { SettingsSection, SettingsTab, SettingsTarget } from "./targets.ts";

/** What this platform has: macOS-only pages and rows are not in the sidebar or the index. */
const CATEGORIES = forPlatform(REGISTRY, IS_MAC);

/** Label and description are getters over the registry's: they follow the display language, never a copy. */
export const SECTIONS: { id: SettingsSection; label: string; description: string; cluster: string }[] =
	CATEGORIES.map((c) => ({
		id: c.id as SettingsSection,
		get label() {
			return c.label;
		},
		get description() {
			return c.description;
		},
		cluster: c.cluster,
	}));

/** The sections that are split in tabs; every other section is one page. */
export const SECTION_TABS: Partial<
	Record<SettingsSection, { id: SettingsTab; label: string; description: string }[]>
> = Object.fromEntries(
	CATEGORIES.filter((c) => c.pages.length > 1).map((c) => [
		c.id,
		c.pages.map((p) => ({
			id: p.id!,
			get label() {
				return p.label!;
			},
			get description() {
				return p.description!;
			},
		})),
	]),
);
