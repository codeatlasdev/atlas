type Entry = { hostKey: string; bot: { id: string } };

/**
 * The bots in the order the Bots sidebar shows them, for ⌘1…⌘9: the first mate (hero) first, then each server's
 * projects in turn. The hero is not repeated when it also sits in a section.
 */
export function shortcutOrder(
	hero: Entry | null,
	sections: { projects: { bots: Entry[] }[] }[],
): { hostKey: string; botId: string }[] {
	const out: { hostKey: string; botId: string }[] = [];
	const seen = new Set<string>();
	const add = (e: Entry) => {
		const k = `${e.hostKey}\u0000${e.bot.id}`;
		if (seen.has(k)) return;
		seen.add(k);
		out.push({ hostKey: e.hostKey, botId: e.bot.id });
	};
	if (hero) add(hero);
	for (const s of sections) for (const p of s.projects) for (const e of p.bots) add(e);
	return out;
}
