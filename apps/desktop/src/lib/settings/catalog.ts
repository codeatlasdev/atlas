/**
 * The searchable list of every user-facing setting, flattened from the registry: where it lives (section, tab,
 * group), its label and description. Data only; matching is in `search.ts`. There is nothing to edit here:
 * add a setting in `registry.ts`.
 */
import { IS_MAC } from "#lib/keys.ts";
import { REGISTRY, forPlatform } from "./registry.ts";
import type { SettingsSection, SettingsTab, SettingsTarget } from "./sections.ts";

export interface SettingEntry extends Omit<SettingsTarget, "item"> {
	id: string;
	label: string;
	description: string;
	/** Extra words people may type that the label does not contain. */
	keywords?: string;
	/** Group title inside the page, shown with search results. */
	group: string;
}

export const SETTINGS_INDEX: SettingEntry[] = forPlatform(REGISTRY, IS_MAC).flatMap((c) =>
	c.pages.flatMap((p) =>
		p.groups.flatMap((g) =>
			g.items.map((i): SettingEntry => ({
				section: c.id as SettingsSection,
				...(p.id && { tab: p.id as SettingsTab }),
				// Getters: the text of a message-backed row (and the group it sits in) follows the display language.
				get group() {
					return g.title;
				},
				id: i.id,
				get label() {
					return i.label;
				},
				get description() {
					return i.description;
				},
				keywords: "keywords" in i ? i.keywords : "",
			})),
		),
	),
);
