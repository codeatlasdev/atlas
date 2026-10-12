/** Lucide icons by name (project icons): all 1.8k nodes, loaded once on first use (~420 KB). */
import { Icon, type LucideIconNode } from "@lucide/svelte";

export type IconNode = LucideIconNode[];

let table: Promise<Record<string, IconNode>> | null = null;

/** The full name → node table, fetched lazily so the main bundle stays small. */
export function lucideTable(): Promise<Record<string, IconNode>> {
	table ??= import("#lib/icon-data/lucide.json").then(
		(m) => m.default as unknown as Record<string, IconNode>,
	);
	return table;
}

export { Icon as LucideIcon };
