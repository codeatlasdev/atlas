/** Small line-oriented dev formats: ignore files, CODEOWNERS, .editorconfig, Procfile, LICENSE. */
import { m } from "#lib/i18n/m.ts";

export interface Line {
	text: string;
	line: number;
}

/** Non-blank, non-comment lines with their numbers; comments directly above a rule are kept as its doc. */
export function ruleLines(text: string): { rule: string; doc: string; line: number }[] {
	const out: { rule: string; doc: string; line: number }[] = [];
	let doc: string[] = [];
	text
		.replace(/\r\n?/g, "\n")
		.split("\n")
		.forEach((raw, i) => {
			const l = raw.trim();
			if (!l) doc = [];
			else if (l.startsWith("#")) doc.push(l.replace(/^#+\s?/, ""));
			else {
				out.push({ rule: l, doc: doc.join(" "), line: i + 1 });
				doc = [];
			}
		});
	return out;
}

export interface IgnoreRule {
	pattern: string;
	negated: boolean;
	dirOnly: boolean;
	/** `/`-anchored to the repo root. */
	anchored: boolean;
	line: number;
}

/** Comment-section headings (`# ---- build ----`) and the rules under them. */
export function ignoreSections(text: string): { title: string; rules: IgnoreRule[] }[] {
	const sections: { title: string; rules: IgnoreRule[] }[] = [{ title: "", rules: [] }];
	let pendingTitle = "";
	text
		.replace(/\r\n?/g, "\n")
		.split("\n")
		.forEach((raw, i) => {
			const l = raw.trim();
			if (!l) {
				if (
					pendingTitle &&
					!sections[sections.length - 1].rules.length &&
					!sections[sections.length - 1].title
				)
					sections[sections.length - 1].title = pendingTitle;
				return;
			}
			if (l.startsWith("#")) {
				const t = l.replace(/^#+\s*[-=*~]*\s*/, "").replace(/\s*[-=*~]*$/, "");
				if (t) {
					pendingTitle = t;
					if (sections[sections.length - 1].rules.length || sections[sections.length - 1].title)
						sections.push({ title: t, rules: [] });
					else sections[sections.length - 1].title = t;
				}
				return;
			}
			const negated = l.startsWith("!");
			let p = negated ? l.slice(1) : l;
			const dirOnly = p.endsWith("/");
			if (dirOnly) p = p.slice(0, -1);
			const anchored = p.startsWith("/");
			sections[sections.length - 1].rules.push({
				pattern: anchored ? p.slice(1) : p,
				negated,
				dirOnly,
				anchored,
				line: i + 1,
			});
		});
	return sections.filter((s) => s.rules.length);
}

export interface Owner {
	pattern: string;
	owners: string[];
	line: number;
}

/** CODEOWNERS: `pattern @user @org/team email`. Later rules win. */
export function parseCodeowners(text: string): { rules: Owner[]; owners: Map<string, number> } {
	const rules: Owner[] = [];
	const owners = new Map<string, number>();
	for (const { rule, line } of ruleLines(text)) {
		const parts = rule.replace(/\s+#.*$/, "").split(/\s+/);
		const pattern = parts.shift() ?? "";
		rules.push({ pattern, owners: parts, line });
		for (const o of parts) owners.set(o, (owners.get(o) ?? 0) + 1);
	}
	return { rules, owners };
}

export interface EditorConfigSection {
	glob: string;
	props: { key: string; value: string; line: number }[];
}

export function parseEditorconfig(text: string): { root: boolean; sections: EditorConfigSection[] } {
	let root = false;
	const sections: EditorConfigSection[] = [];
	text
		.replace(/\r\n?/g, "\n")
		.split("\n")
		.forEach((raw, i) => {
			const l = raw.trim();
			if (!l || l.startsWith("#") || l.startsWith(";")) return;
			const s = /^\[(.+)\]$/.exec(l);
			if (s) {
				sections.push({ glob: s[1], props: [] });
				return;
			}
			const kv = /^([^=]+?)\s*=\s*(.*)$/.exec(l);
			if (!kv) return;
			const key = kv[1].toLowerCase();
			if (!sections.length) {
				if (key === "root") root = kv[2].toLowerCase() === "true";
				return;
			}
			sections[sections.length - 1].props.push({ key, value: kv[2], line: i + 1 });
		});
	return { root, sections };
}

/** Plain-language meaning of an editorconfig key, in the display language; empty for a key Atlas has no words for. */
export function editorconfigDoc(key: string): string {
	switch (key) {
		case "indent_style":
			return m.viewers_reference_editorconfig_indent_style();
		case "indent_size":
			return m.viewers_reference_editorconfig_indent_size();
		case "tab_width":
			return m.viewers_reference_editorconfig_tab_width();
		case "end_of_line":
			return m.viewers_reference_editorconfig_end_of_line();
		case "charset":
			return m.viewers_reference_editorconfig_charset();
		case "trim_trailing_whitespace":
			return m.viewers_reference_editorconfig_trim_trailing_whitespace();
		case "insert_final_newline":
			return m.viewers_reference_editorconfig_insert_final_newline();
		case "max_line_length":
			return m.viewers_reference_editorconfig_max_line_length();
		default:
			return "";
	}
}

export interface ProcEntry {
	name: string;
	command: string;
	line: number;
}

export function parseProcfile(text: string): ProcEntry[] {
	return ruleLines(text).flatMap(({ rule, line }) => {
		const entry = /^([A-Za-z0-9_-]+)\s*:\s*(.*)$/.exec(rule);
		return entry ? [{ name: entry[1], command: entry[2], line }] : [];
	});
}

export interface LicenseInfo {
	spdx: string | null;
	name: string;
	holder: string;
	year: string;
	/** What the license lets you do, as short phrases. */
	summary: string[];
}

/** What a license lets you do or asks of you: the words are read when shown, so they follow the display language. */
const TERMS = {
	commercial: () => m.viewers_reference_license_commercial_use(),
	modification: () => m.viewers_reference_license_modification(),
	distribution: () => m.viewers_reference_license_distribution(),
	private: () => m.viewers_reference_license_private_use(),
	keepNotice: () => m.viewers_reference_license_keep_notice(),
	patent: () => m.viewers_reference_license_patent_grant(),
	stateChanges: () => m.viewers_reference_license_state_changes(),
	copyleft: () => m.viewers_reference_license_copyleft_share_source(),
	sameLicense: () => m.viewers_reference_license_same_license(),
	networkCopyleft: () => m.viewers_reference_license_network_copyleft(),
	shareSource: () => m.viewers_reference_license_share_source(),
	linkAny: () => m.viewers_reference_license_link_from_any_code(),
	shareLibrary: () => m.viewers_reference_license_share_library_changes(),
	fileCopyleft: () => m.viewers_reference_license_file_level_copyleft(),
	noEndorsement: () => m.viewers_reference_license_no_endorsement(),
	publicDomain: () => m.viewers_reference_license_public_domain(),
	anyUse: () => m.viewers_reference_license_any_use(),
	publicDomainDedication: () => m.viewers_reference_license_public_domain_dedication(),
};

const LICENSES: { spdx: string; name: string; re: RegExp; summary: (keyof typeof TERMS)[] }[] = [
	{
		spdx: "MIT",
		name: "MIT License",
		re: /permission is hereby granted, free of charge/i,
		summary: ["commercial", "modification", "distribution", "private", "keepNotice"],
	},
	{
		spdx: "Apache-2.0",
		name: "Apache License 2.0",
		re: /apache license[\s\S]{0,40}version 2\.0/i,
		summary: ["commercial", "modification", "patent", "stateChanges", "keepNotice"],
	},
	{
		spdx: "GPL-3.0-only",
		name: "GNU GPL v3",
		re: /gnu general public license[\s\S]{0,60}version 3/i,
		summary: ["commercial", "copyleft", "patent", "sameLicense"],
	},
	{
		spdx: "GPL-2.0-only",
		name: "GNU GPL v2",
		re: /gnu general public license[\s\S]{0,60}version 2/i,
		summary: ["commercial", "copyleft", "sameLicense"],
	},
	{
		spdx: "AGPL-3.0-only",
		name: "GNU AGPL v3",
		re: /gnu affero general public license/i,
		summary: ["networkCopyleft", "shareSource", "sameLicense"],
	},
	{
		spdx: "LGPL-3.0-only",
		name: "GNU LGPL v3",
		re: /gnu lesser general public license/i,
		summary: ["linkAny", "shareLibrary"],
	},
	{
		spdx: "MPL-2.0",
		name: "Mozilla Public License 2.0",
		re: /mozilla public license[\s\S]{0,40}2\.0/i,
		summary: ["commercial", "fileCopyleft", "patent"],
	},
	{
		spdx: "BSD-3-Clause",
		name: "BSD 3-Clause",
		re: /neither the name of[\s\S]{0,200}nor the names of its contributors/i,
		summary: ["commercial", "modification", "noEndorsement", "keepNotice"],
	},
	{
		spdx: "BSD-2-Clause",
		name: "BSD 2-Clause",
		re: /redistribution and use in source and binary forms[\s\S]{0,400}this list of conditions/i,
		summary: ["commercial", "modification", "keepNotice"],
	},
	{
		spdx: "ISC",
		name: "ISC License",
		re: /permission to use, copy, modify, and\/or distribute this software/i,
		summary: ["commercial", "modification", "keepNotice"],
	},
	{
		spdx: "Unlicense",
		name: "The Unlicense",
		re: /this is free and unencumbered software released into the public domain/i,
		summary: ["publicDomain", "anyUse"],
	},
	{ spdx: "CC0-1.0", name: "CC0 1.0", re: /cc0 1\.0 universal/i, summary: ["publicDomainDedication"] },
];

export function detectLicense(text: string): LicenseInfo {
	const hit = LICENSES.find((l) => l.re.test(text));
	const c = /copyright\s*(?:\(c\)|©)?\s*(\d{4}(?:\s*[-–,]\s*\d{4})?)?\s*(.*)/i.exec(text);
	return {
		spdx: hit?.spdx ?? null,
		name: hit?.name ?? m.viewers_reference_license_unrecognized(),
		holder: c?.[2]?.trim().replace(/\.$/, "") ?? "",
		year: c?.[1] ?? "",
		summary: (hit?.summary ?? []).map((term) => TERMS[term]()),
	};
}
