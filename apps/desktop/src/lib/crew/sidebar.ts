/**
 * Crew sidebar grouping (pure): the first mate ("favorite") chosen per crew server and persisted in localStorage
 * `crew:favorite`, and the list sections (crew server → project → crewmates) built from the flat crew list.
 */
import type { Crewmate } from "#lib/crew.ts";
import { crewmateKey } from "#lib/crew/crewmateKey.ts";

export const FAVORITE_KEY = "crew:favorite";

export type Entry = { hostKey: string; hostName?: string; crewmate: Crewmate };

export type ProjectGroup = {
	projectName: string | null;
	crewmates: { hostKey: string; crewmate: Crewmate }[];
};

export type Section = {
	hostKey: string;
	hostName: string;
	projects: ProjectGroup[];
};

/** Favorites map `hostKey → crewmateId`, in insertion order (the last one set wins the hero slot). */
export function loadFavorites(
	storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage,
): Map<string, string> {
	try {
		return new Map(Object.entries(JSON.parse(storage?.getItem(FAVORITE_KEY) ?? "{}")));
	} catch {
		return new Map();
	}
}

export function saveFavorites(
	m: Map<string, string>,
	storage: Pick<Storage, "setItem"> | undefined = globalThis.localStorage,
) {
	try {
		storage?.setItem(FAVORITE_KEY, JSON.stringify(Object.fromEntries(m)));
	} catch {
		/* storage full or unavailable */
	}
}

/** A copy of `favs` with `crewmateId` the favorite of `hostKey`, moved last so it wins the hero slot. */
export function withFavorite(
	favs: Map<string, string>,
	hostKey: string,
	crewmateId: string,
): Map<string, string> {
	const m = new Map(favs);
	m.delete(hostKey);
	m.set(hostKey, crewmateId);
	return m;
}

/** The hero: the first stored favorite that still exists, otherwise the first crewmate; null when there is none. */
export function favoriteOf<T extends { hostKey: string; crewmate: { id: string } }>(
	favs: Map<string, string>,
	all: T[],
): T | null {
	for (const [hk, id] of favs) {
		const e = all.find((x) => x.hostKey === hk && x.crewmate.id === id);
		if (e) return e;
	}
	return all[0] ?? null;
}

/** Group the flat crew list by crew server, then by project (unassigned crewmates form the `null` project). */
export function groupSections(all: Entry[], servers: { key: string; name: string }[]): Section[] {
	const map = new Map<string, Section>();
	for (const entry of all) {
		const host = servers.find((h) => h.key === entry.hostKey);
		if (!map.has(entry.hostKey)) {
			map.set(entry.hostKey, {
				hostKey: entry.hostKey,
				hostName: host?.name ?? "This computer",
				projects: [],
			});
		}
		const section = map.get(entry.hostKey)!;
		const projectName = entry.crewmate.project?.name ?? null;
		let pg = section.projects.find((p) => p.projectName === projectName);
		if (!pg) {
			pg = { projectName, crewmates: [] };
			section.projects.push(pg);
		}
		pg.crewmates.push({ hostKey: entry.hostKey, crewmate: entry.crewmate });
	}
	return [...map.values()];
}

/** The sections as the list shows them: only `shown` crewmates (the hero is left out), and no empty server/project. */
export function visibleSections(
	sections: Section[],
	shown: { hostKey: string; crewmate: { id: string } }[],
): Section[] {
	const keys = new Set(shown.map((x) => crewmateKey(x.hostKey, x.crewmate.id)));
	return sections
		.map((s) => ({
			...s,
			projects: s.projects
				.map((p) => ({
					...p,
					crewmates: p.crewmates.filter((c) => keys.has(crewmateKey(c.hostKey, c.crewmate.id))),
				}))
				.filter((p) => p.crewmates.length > 0),
		}))
		.filter((s) => s.projects.length > 0);
}
