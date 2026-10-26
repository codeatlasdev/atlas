/**
 * Settings navigation derived from the registry: which sections and tabs exist, and how any id (a section, a
 * `section/tab`, a row id, or an alias that shipped before) resolves to where it lives now.
 * Pure: no Svelte, no IPC.
 */
import { REGISTRY, type RegistryDefs } from "./registry.ts";

type Registry = RegistryDefs;
export type SettingsSection = Registry[number]["id"];
/** Every tab id of every tabbed section (`editor`, `languages`, `shell`, …). */
export type SettingsTab = Extract<Registry[number]["pages"][number], { id: string }>["id"];

/** Where Settings opens: a section, a tab when the section has tabs, and a row to reveal. */
export interface SettingsTarget {
	section: SettingsSection;
	tab?: SettingsTab;
	/** Row id (`data-setting`) to scroll to and flash once the page renders. */
	item?: string;
}

const CATEGORIES = REGISTRY;

/** The registry's ids are the literal `SettingsSection` union by construction (derived from the same data). */
const sectionId = (id: string) => id as SettingsSection;

export const SECTIONS: { id: SettingsSection; label: string; description: string; cluster: string }[] =
	CATEGORIES.map(({ id, label, description, cluster }) => ({
		id: sectionId(id),
		label,
		description,
		cluster,
	}));

/** The sections that are split in tabs; every other section is one page. */
export const SECTION_TABS: Partial<
	Record<SettingsSection, { id: SettingsTab; label: string; description: string }[]>
> = Object.fromEntries(
	CATEGORIES.filter((c) => c.pages.length > 1).map((c) => [
		c.id,
		c.pages.map((p) => ({ id: p.id!, label: p.label!, description: p.description! })),
	]),
);

/** Every id that opens something, mapped to its target: sections, `section/tab` pairs, aliases, then rows. */
const LINKS = new Map<string, SettingsTarget>();
const link = (id: string, t: SettingsTarget) => {
	if (LINKS.has(id)) throw new Error(`Settings registry: "${id}" opens two places`);
	LINKS.set(id, t);
};
for (const c of CATEGORIES) {
	const first = c.pages[0]!;
	const open = (p: (typeof c.pages)[number]) => ({
		section: sectionId(c.id),
		...(p.id && { tab: p.id as SettingsTab }),
	});
	link(c.id, open(first));
	for (const a of c.aliases ?? []) link(a, open(first));
	for (const p of c.pages) {
		if (p.id) link(`${c.id}/${p.id}`, open(p));
		for (const a of p.aliases ?? []) link(a, open(p));
		for (const g of p.groups)
			for (const i of g.items) {
				link(i.id, { ...open(p), item: i.id });
				for (const a of i.aliases ?? []) link(a, { ...open(p), item: i.id });
			}
	}
}

/**
 * Accepts any id the app has ever opened Settings with and returns where it lives now: a section id
 * (`terminal`), a `section/tab` pair (`editor/languages`; an unknown tab opens the first), a row id (`shell`,
 * which also names the row to reveal) or an alias (`appearance/terminal`). Unknown ids open the first section.
 */
export function resolveSection(id: string): SettingsTarget {
	const exact = LINKS.get(id);
	if (exact) return { ...exact };
	const [head = "", sub, ...rest] = id.split("/");
	const category = rest.length === 0 && sub ? CATEGORIES.find((c) => c.id === head) : undefined;
	if (category) {
		const target = LINKS.get(category.id)!;
		return category.pages.length > 1
			? { section: target.section, tab: target.tab }
			: { section: target.section };
	}
	return { section: sectionId(CATEGORIES[0]!.id) };
}

/** Where a sidebar click lands: the section and its first tab. */
export const openSection = (section: SettingsSection): SettingsTarget => ({ ...LINKS.get(section)! });
