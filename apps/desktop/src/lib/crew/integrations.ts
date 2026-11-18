/**
 * Integration catalog data.
 *
 * Adding a new real integration = one entry in CATALOG with status: "real"
 * and wiring its connection logic into IntegrationsCatalog.svelte.
 *
 * The words are catalog messages read through getters, so a language switch reaches a catalog built once.
 */
import { m } from "#lib/i18n/m.ts";

export type IntegrationCategory = "Code" | "Productivity" | "Communication" | "Design" | "Password managers";

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

/** The chip's text for a category (the category ids stay English: they are keys, not words). */
export function categoryLabel(category: CatalogCategory): string {
	switch (category) {
		case "All":
			return m.crew_workspace_integrations_category_all();
		case "Code":
			return m.crew_workspace_integrations_category_code();
		case "Productivity":
			return m.crew_workspace_integrations_category_productivity();
		case "Communication":
			return m.crew_workspace_integrations_category_communication();
		case "Design":
			return m.crew_workspace_integrations_category_design();
		case "Password managers":
			return m.crew_workspace_integrations_category_password_managers();
	}
}

export const CATALOG: Integration[] = [
	{
		id: "github",
		name: "GitHub",
		category: "Code",
		get description() {
			return m.crew_workspace_integrations_github_description();
		},
		status: "real",
		sourceUrl: "https://github.com/codeatlasdev",
		skills: [
			{
				get label() {
					return m.crew_workspace_integrations_github_skill_clone();
				},
				available: true,
			},
			{
				get label() {
					return m.crew_workspace_integrations_github_skill_open_prs();
				},
				available: false,
			},
			{
				get label() {
					return m.crew_workspace_integrations_github_skill_comment_prs();
				},
				available: false,
			},
			{
				get label() {
					return m.crew_workspace_integrations_github_skill_ci_status();
				},
				available: false,
			},
		],
	},
	{
		id: "linear",
		name: "Linear",
		category: "Productivity",
		get description() {
			return m.crew_workspace_integrations_linear_description();
		},
		status: "coming-soon",
		skills: [],
	},
	{
		id: "notion",
		name: "Notion",
		category: "Productivity",
		get description() {
			return m.crew_workspace_integrations_notion_description();
		},
		status: "coming-soon",
		skills: [],
	},
	{
		id: "slack",
		name: "Slack",
		category: "Communication",
		get description() {
			return m.crew_workspace_integrations_slack_description();
		},
		status: "coming-soon",
		skills: [],
	},
	{
		id: "figma",
		name: "Figma",
		category: "Design",
		get description() {
			return m.crew_workspace_integrations_figma_description();
		},
		status: "coming-soon",
		skills: [],
	},
	{
		id: "1password",
		name: "1Password",
		category: "Password managers",
		get description() {
			return m.crew_workspace_integrations_1password_description();
		},
		status: "coming-soon",
		skills: [],
	},
	{
		id: "google-docs",
		name: "Google Docs",
		category: "Productivity",
		get description() {
			return m.crew_workspace_integrations_google_docs_description();
		},
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
