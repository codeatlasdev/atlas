/**
 * Which features a file can afford. A file is `large` from the byte threshold (default 2 MB),
 * `huge` from 5x that; a single very long line (minified bundles) is `large` above 10k chars and
 * `huge` above 200k, whatever the byte size. Pure: the integrator turns `disable` into extensions.
 */
import { formatNumber } from "#lib/i18n/fmt.ts";

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
	reasons: string[];
	disable: Disable;
	notice: string | null;
}

const NOTICES: Record<Tier, string | null> = {
	normal: null,
	large: "Large file: word wrap, language features and other heavy extras are off so the editor stays fast.",
	huge: "Huge file: syntax highlighting and language features are off so the editor stays fast.",
};

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

/** Tier, the reasons for it, the features to turn off and the one-line notice for the user. */
export function classifyFile(m: FileMetrics, t: Partial<Thresholds> = {}): Classification {
	const th = { ...DEFAULT_THRESHOLDS, ...t };
	let tier: Tier = "normal";
	const reasons: string[] = [];
	const raise = (to: Tier, reason: string) => {
		if (RANK[to] > RANK[tier]) tier = to;
		reasons.push(reason);
	};
	if (m.bytes >= th.largeBytes * th.hugeFactor) raise("huge", `${mb(m.bytes)} file`);
	else if (m.bytes >= th.largeBytes) raise("large", `${mb(m.bytes)} file`);
	if (m.longestLine > th.hugeLine) raise("huge", `a line has ${formatNumber(m.longestLine)} characters`);
	else if (m.longestLine > th.longLine)
		raise("large", `a line has ${formatNumber(m.longestLine)} characters`);
	return { tier, reasons, disable: { ...DISABLE[tier] }, notice: NOTICES[tier] };
}
