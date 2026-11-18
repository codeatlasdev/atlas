/**
 * Which features a file can afford. A file is `large` from the byte threshold (default 2 MB),
 * `huge` from 5x that; a single very long line (minified bundles) is `large` above 10k chars and
 * `huge` above 200k, whatever the byte size. Pure: the integrator turns `disable` into extensions.
 */
import { formatNumber } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";

export type Tier = "normal" | "large" | "huge";

export interface FileMetrics {
	bytes: number;
	lines: number;
	longestLine: number;
}

export interface Thresholds {
	/** `large_file_threshold_bytes` pref: the size where the large tier starts. */
	largeBytes: number;
	/** Longest line (chars) allowed before the large tier. */
	longLine: number;
	/** Longest line (chars) allowed before the huge tier. */
	hugeLine: number;
	/** Multiple of `largeBytes` where the huge tier starts. */
	hugeFactor: number;
}

export const DEFAULT_THRESHOLDS: Readonly<Thresholds> = {
	largeBytes: 2_000_000,
	longLine: 10_000,
	hugeLine: 200_000,
	hugeFactor: 5,
};

export interface Disable {
	wrap: boolean;
	bracketColors: boolean;
	indentGuides: boolean;
	minimap: boolean;
	stickyScroll: boolean;
	lsp: boolean;
	foldGutter: boolean;
	/** `full` and `viewport` use the grammar (CodeMirror parses around the viewport on demand), `none` is plain text. */
	syntax: "full" | "viewport" | "none";
	inlayHints: boolean;
	selectionMatches: boolean;
}

export interface Classification {
	tier: Tier;
	/** Why, for diagnostics and tests; never shown (the person reads `largeNotice(tier)`). */
	reasons: string[];
	disable: Disable;
}

/** The one-line notice for the user, in the app's language; `null` for a normal file. */
export function largeNotice(tier: Tier): string | null {
	if (tier === "large") return m.workbench_editor_notice_large();
	return tier === "huge" ? m.workbench_editor_notice_huge() : null;
}

const DISABLE: Record<Tier, Disable> = {
	normal: {
		wrap: false,
		bracketColors: false,
		indentGuides: false,
		minimap: false,
		stickyScroll: false,
		lsp: false,
		foldGutter: false,
		syntax: "full",
		inlayHints: false,
		selectionMatches: false,
	},
	large: {
		wrap: true,
		bracketColors: true,
		indentGuides: true,
		minimap: true,
		stickyScroll: true,
		lsp: true,
		foldGutter: false,
		syntax: "viewport",
		inlayHints: true,
		selectionMatches: true,
	},
	huge: {
		wrap: true,
		bracketColors: true,
		indentGuides: true,
		minimap: true,
		stickyScroll: true,
		lsp: true,
		foldGutter: true,
		syntax: "none",
		inlayHints: true,
		selectionMatches: true,
	},
};

const RANK: Record<Tier, number> = { normal: 0, large: 1, huge: 2 };

const mb = (bytes: number) => `${(bytes / 1_000_000).toFixed(1)} MB`;

/** Tier, the reasons for it and the features to turn off. */
export function classifyFile(metrics: FileMetrics, t: Partial<Thresholds> = {}): Classification {
	const th = { ...DEFAULT_THRESHOLDS, ...t };
	let tier: Tier = "normal";
	const reasons: string[] = [];
	const raise = (to: Tier, reason: string) => {
		if (RANK[to] > RANK[tier]) tier = to;
		reasons.push(reason);
	};
	if (metrics.bytes >= th.largeBytes * th.hugeFactor) raise("huge", `${mb(metrics.bytes)} file`);
	else if (metrics.bytes >= th.largeBytes) raise("large", `${mb(metrics.bytes)} file`);
	if (metrics.longestLine > th.hugeLine) raise("huge", `a line has ${formatNumber(metrics.longestLine)} characters`);
	else if (metrics.longestLine > th.longLine)
		raise("large", `a line has ${formatNumber(metrics.longestLine)} characters`);
	return { tier, reasons, disable: { ...DISABLE[tier] } };
}
