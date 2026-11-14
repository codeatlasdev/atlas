import { crewmateKey } from "#lib/crew/crewmateKey.ts";

type Entry = { hostKey: string; crewmate: { id: string } };

/**
 * The crewmates in the order the Crew sidebar shows them, for ⌘1…⌘9: the first mate (hero) first, then each server's
 * projects in turn. The hero is not repeated when it also sits in a section.
 */
export function shortcutOrder(
	hero: Entry | null,
	sections: { projects: { crewmates: Entry[] }[] }[],
): { hostKey: string; crewmateId: string }[] {
	const out: { hostKey: string; crewmateId: string }[] = [];
	const seen = new Set<string>();
	const add = (e: Entry) => {
		const k = crewmateKey(e.hostKey, e.crewmate.id);
		if (seen.has(k)) return;
		seen.add(k);
		out.push({ hostKey: e.hostKey, crewmateId: e.crewmate.id });
	};
	if (hero) add(hero);
	for (const s of sections) for (const p of s.projects) for (const e of p.crewmates) add(e);
	return out;
}
