/**
 * The Settings registry: the ONE declarative description of what Settings contains.
 *
 *   category (sidebar entry, page title) -> page (a tab, or the whole page) -> group (a card) -> item (a row)
 *
 * It is two files joined here: the structure (`registryIds.ts`: ids, nesting, aliases, platform; loads at startup for
 * `updates.show("<id>")`) and the text (`registryText.ts`: labels, descriptions, group titles, keywords; loads with the
 * Settings view). The sidebar, the page header, the tab strip, the search index, the breadcrumb of a result and every
 * deep link are all derived from them, so they cannot disagree. Adding a setting is one entry in each plus its row in
 * the page component named in `files`; `registry.test.ts` and `placement.test.ts` go red when a half is missing.
 *
 * Rules the tests pin:
 *  - ids never change when a setting moves (search, deep links and the flash on arrival key on them);
 *  - an `aliases` entry keeps an id that shipped before working after the thing it named moved or was renamed;
 *  - labels and descriptions are identical to the screens;
 *  - every user-visible text is a message read through a getter, so the sidebar, the page, the tab strip and the
 *    search follow the display language live (nothing here may copy a label into a plain string at module load).
 * Pure: no Svelte, no IPC. Icons and components live next to the UI (`SettingsNav`, `pageComponents.ts`).
 */
import { m } from "#lib/i18n/m.ts";
import { IDS, type CategoryIds, type ItemIds, type PageIds, type RegistryDefs } from "./registryIds.ts";
import { CATEGORY_TEXT, GROUP_TITLE, ITEM_TEXT, PAGE_TEXT } from "./registryText.ts";

export type { RegistryDefs };

/** One row. `keywords` are words people may type that the label does not contain. */
export interface ItemDef {
	readonly id: string;
	readonly label: string;
	readonly description: string;
	readonly keywords?: string;
	/** Older deep-link ids for this row. */
	readonly aliases?: readonly string[];
	/** Only exists on macOS (the native GPU terminal, capture): hidden from nav, search and deep links elsewhere. */
	readonly macOnly?: true;
}

/** A card of rows under a title. */
export interface GroupDef {
	readonly title: string;
	readonly items: readonly ItemDef[];
}

/** A tab of a category, or (without `id`/`label`) the whole page of a single-page category. */
export interface PageDef {
	readonly id?: string;
	readonly label?: string;
	/** One line under the tab strip. Required for tabs. */
	readonly description?: string;
	readonly aliases?: readonly string[];
	/** Sources that render this page, relative to `src/lib`; `placement.test.ts` reads them. */
	readonly files: readonly string[];
	readonly groups: readonly GroupDef[];
}

export interface CategoryDef {
	readonly id: string;
	readonly label: string;
	/** One line: the page header, and the tooltip of the sidebar entry. */
	readonly description: string;
	/** Sidebar heading the entry sits under. */
	readonly cluster: "app" | "work" | "connect";
	/** Older deep-link ids for the whole category (`appearance/terminal`). */
	readonly aliases?: readonly string[];
	/** The whole category is macOS only (capture): no sidebar entry, no search hits, deep links fall back. */
	readonly macOnly?: true;
	readonly pages: readonly PageDef[];
}

/**
 * Sidebar headings, in order. The label is a getter: it is read when the sidebar draws, so it follows the display
 * language.
 */
export const CLUSTERS = [
	{
		id: "app",
		get label() {
			return m.settings_registry_clusters_app();
		},
	},
	{
		id: "work",
		get label() {
			return m.settings_registry_clusters_work();
		},
	},
	{
		id: "connect",
		get label() {
			return m.settings_registry_clusters_connect();
		},
	},
] as const;

/** The text of an id the structure lists: a missing entry is a bug the registry tests name, not a blank row. */
function textOf<T>(table: Readonly<Record<string, T>>, key: string): T {
	const t = table[key];
	if (!t) throw new Error(`Settings registry: no text for "${key}" in registryText.ts`);
	return t;
}

function item(i: ItemIds): ItemDef {
	const t = textOf(ITEM_TEXT, i.id);
	return {
		id: i.id,
		get label() {
			return t.label();
		},
		get description() {
			return t.description();
		},
		...(t.keywords && { keywords: t.keywords }),
		...(i.aliases && { aliases: i.aliases }),
		...(i.macOnly && { macOnly: i.macOnly }),
	};
}

function page(c: CategoryIds, p: PageIds, pi: number): PageDef {
	const t = p.id ? textOf(PAGE_TEXT, `${c.id}/${p.id}`) : undefined;
	return {
		...(p.id && { id: p.id }),
		...(t && {
			get label() {
				return t.label();
			},
			get description() {
				return t.description();
			},
		}),
		...(p.aliases && { aliases: p.aliases }),
		files: p.files,
		groups: p.groups.map((g, gi) => {
			const title = textOf(GROUP_TITLE, `${c.id}#${pi}#${gi}`);
			return {
				get title() {
					return title();
				},
				items: g.items.map(item),
			};
		}),
	};
}

function category(c: CategoryIds): CategoryDef {
	const t = textOf(CATEGORY_TEXT, c.id);
	return {
		id: c.id,
		get label() {
			return t.label();
		},
		get description() {
			return t.description();
		},
		cluster: c.cluster,
		...(c.aliases && { aliases: c.aliases }),
		...(c.macOnly && { macOnly: c.macOnly }),
		pages: c.pages.map((p, pi) => page(c, p, pi)),
	};
}

/** The registry as plain definitions, for code that walks it without needing the literal ids. Every platform's rows. */
export const REGISTRY: readonly CategoryDef[] = IDS.map(category);

/**
 * `base` with some properties replaced, keeping its getters live. Not `{ ...base, ...over }`: a spread would freeze a
 * message-backed label, description or title at its current language.
 */
const override = <T extends object>(base: T, over: object): T =>
	Object.defineProperties(
		{},
		{ ...Object.getOwnPropertyDescriptors(base), ...Object.getOwnPropertyDescriptors(over) },
	) as T;

/**
 * What this platform shows: the registry without the categories and rows that only exist on macOS (capture, the
 * native GPU terminal and its own options). Sidebar, search index and deep links all read this, so a hidden page can
 * neither be found nor opened. Empty groups and pages drop with their last row.
 */
export function forPlatform(defs: readonly CategoryDef[], mac: boolean): readonly CategoryDef[] {
	if (mac) return defs;
	return defs
		.filter((c) => !c.macOnly)
		.map((c) =>
			override(c, {
				pages: c.pages
					.map((p) =>
						override(p, {
							groups: p.groups
								.map((g) => override(g, { items: g.items.filter((i) => !i.macOnly) }))
								.filter((g) => g.items.length > 0),
						}),
					)
					.filter((p) => p.groups.length > 0),
			}),
		);
}
