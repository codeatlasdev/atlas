/**
 * The searchable list of every user-facing setting, flattened from the registry: where it lives (section, tab,
 * group), its label and description. Data only; matching is in `search.ts`. There is nothing to edit here:
 * add a setting in `registry.ts`.
 */
import { REGISTRY } from "./registry.ts";
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

export const SETTINGS_INDEX: SettingEntry[] = REGISTRY.flatMap((c) =>
	c.pages.flatMap((p) =>
		p.groups.flatMap((g) =>
			g.items.map((i): SettingEntry => ({
				section: c.id as SettingsSection,
				...(p.id && { tab: p.id as SettingsTab }),
				group: g.title,
				id: i.id,
				label: i.label,
				description: i.description,
				keywords: "keywords" in i ? i.keywords : "",
			})),
		),
	),
);
