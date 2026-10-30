/**
 * Integration catalog data.
 *
 * Adding a new real integration = one entry in CATALOG with status: "real"
 * and wiring its connection logic into IntegrationsCatalog.svelte.
 */

export type IntegrationCategory =
	| "Code"
	| "Productivity"
	| "Communication"
	| "Design"
	| "Password managers";

export type IntegrationStatus = "real" | "coming-soon";

export interface IntegrationSkill {
	label: string;
	/** false = displayed as "Coming soon" */
	available: boolean;
}

export interface Integration {
	id: string;
	name: string;
	category: IntegrationCategory;
	description: string;
	status: IntegrationStatus;
	/** Link shown as "View source ↗" on the detail page. Only for real integrations. */
	sourceUrl?: string;
	skills: IntegrationSkill[];
}

export const CATEGORIES = [
	"All",
	"Code",
	"Productivity",
	"Communication",
	"Design",
	"Password managers",
] as const;

export type CatalogCategory = (typeof CATEGORIES)[number];

export const CATALOG: Integration[] = [
	{
		id: "github",
		name: "GitHub",
		category: "Code",
		description: "Give the crew access to your private repositories.",
		status: "real",
		sourceUrl: "https://github.com/codeatlasdev",
		skills: [
			{ label: "Clone private repos", available: true },
			{ label: "Open pull requests", available: false },
			{ label: "Comment on pull requests", available: false },
			{ label: "Read CI status", available: false },
		],
	},
	{
		id: "linear",
		name: "Linear",
		category: "Productivity",
		description: "Create and update issues from the crew's tasks.",
		status: "coming-soon",
		skills: [],
	},
	{
		id: "notion",
		name: "Notion",
		category: "Productivity",
		description: "Read and write Notion pages as part of crew tasks.",
		status: "coming-soon",
		skills: [],
	},
	{
		id: "slack",
		name: "Slack",
		category: "Communication",
		description: "Send messages and read channel history.",
		status: "coming-soon",
		skills: [],
	},
	{
		id: "figma",
		name: "Figma",
		category: "Design",
		description: "Read design files and export assets.",
		status: "coming-soon",
		skills: [],
	},
	{
		id: "1password",
		name: "1Password",
		category: "Password managers",
		description: "Securely share credentials with the crew.",
		status: "coming-soon",
		skills: [],
	},
	{
		id: "google-docs",
		name: "Google Docs",
		category: "Productivity",
		description: "Read and edit documents in the crew's workspace.",
		status: "coming-soon",
		skills: [],
	},
];

// ─── Pure helpers ───────────────────────────────────────────────────────────

/** Filter catalog entries by a free-text query and category chip selection. */
export function filterCatalog(
	catalog: Integration[],
	query: string,
	category: CatalogCategory,
): Integration[] {
	const q = query.trim().toLowerCase();
	return catalog.filter((i) => {
		if (category !== "All" && i.category !== category) return false;
		if (!q) return true;
		return i.name.toLowerCase().includes(q) || i.description.toLowerCase().includes(q);
	});
}

/**
 * Returns the set of integration IDs that are currently connected.
 * Kept as a pure function so tests can drive it without the Svelte store.
 */
export function connectedIntegrationIds(githubConnected: boolean): Set<string> {
	const ids = new Set<string>();
	if (githubConnected) ids.add("github");
	return ids;
}

/** True only if the integration can be actively connected (not coming-soon). */
export function canConnect(integration: Integration): boolean {
	return integration.status === "real";
}
