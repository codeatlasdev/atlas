/**
 * Project identity: the icon and color a project shows everywhere (sidebar, breadcrumb, tray,
 * palette, explorer). Pure: resolution order, repo-logo detection list, monogram, validation.
 *
 * Icon sources, strongest first:
 *   1. the user's choice (emoji, Lucide icon, or an image inside the repo),
 *   2. the repo's own logo (first hit of `LOGO_CANDIDATES`, or package.json `icon`),
 *   3. a monogram on the accent color.
 */
import { hueOf } from "#lib/shell/space.ts";

export type IconChoice =
	| { kind: "emoji"; value: string }
	| { kind: "lucide"; value: string }
	/** Path relative to the project root. */
	| { kind: "image"; value: string };

/** What `projects_set_identity` stores (`registry::Identity`). Both parts optional. */
export interface Identity {
	icon?: IconChoice;
	hue?: number;
}

/** What a project shows: rendered by `ProjectIcon.svelte`. */
export type ResolvedIcon =
	| { kind: "emoji"; value: string }
	| { kind: "lucide"; value: string }
	/** `src` is a displayable URL (object/data URL) of an image read from the repo. */
	| { kind: "image"; src: string; auto: boolean }
	| { kind: "monogram"; text: string };

/**
 * Where a repo usually keeps its logo, most specific first. The scan stops at the first file
 * that exists and decodes; `package.json` `icon` (a relative path) is tried before these.
 * Raster formats come after vector at the same location: SVG scales cleanly in a 16–28px slot.
 */
export const LOGO_CANDIDATES: readonly string[] = [
	"public/favicon.svg",
	"public/icon.svg",
	"public/logo.svg",
	"static/favicon.svg",
	"static/logo.svg",
	"app/icon.svg",
	"src/app/icon.svg",
	"app/icon.png",
	"src/app/icon.png",
	"app/icon.ico",
	"src/app/icon.ico",
	"assets/logo.svg",
	"assets/icon.svg",
	"assets/logo.png",
	"assets/icon.png",
	".github/logo.svg",
	".github/logo.png",
	"logo.svg",
	"logo.png",
	"icon.svg",
	"icon.png",
	"public/apple-touch-icon.png",
	"public/favicon.png",
	"static/favicon.png",
	"public/logo.png",
	"public/icon.png",
	"public/favicon.ico",
	"static/favicon.ico",
	"favicon.ico",
];

/** Raster/vector types a webview shows from a Blob, by extension. */
export const IMAGE_MIME: Record<string, string> = {
	svg: "image/svg+xml",
	png: "image/png",
	jpg: "image/jpeg",
	jpeg: "image/jpeg",
	webp: "image/webp",
	gif: "image/gif",
	ico: "image/x-icon",
};

export const mimeOf = (path: string): string | null =>
	IMAGE_MIME[path.slice(path.lastIndexOf(".") + 1).toLowerCase()] ?? null;

/** Largest logo file read for the sidebar (a 28px slot never needs more). */
export const MAX_LOGO_BYTES = 256 * 1024;

/**
 * Candidate paths for detection: package.json's `icon` (when it is a plain relative image
 * path) first, then `LOGO_CANDIDATES`. Deduplicated, order kept.
 */
export function detectionPaths(packageJson: string | null): string[] {
	const out: string[] = [];
	if (packageJson) {
		try {
			const pkg: unknown = JSON.parse(packageJson);
			const icon = pkg && typeof pkg === "object" && "icon" in pkg ? pkg.icon : null;
			if (typeof icon === "string" && isRepoPath(icon) && mimeOf(icon)) out.push(icon.replace(/^\.\//, ""));
		} catch {
			// not JSON: ignore, the fixed list still applies
		}
	}
	for (const c of LOGO_CANDIDATES) if (!out.includes(c)) out.push(c);
	return out;
}

/** A path that stays inside the project root (no absolute path, no `..`). */
export function isRepoPath(p: string): boolean {
	return !!p && !p.startsWith("/") && !p.split("/").includes("..");
}

/**
 * One or two letters for the monogram. Words split on separators and camelCase; two or more
 * words → first letter of the first two (`orbit-shop` → `OS`); one word → its first letter
 * (`atlas` → `A`; no guessing at acronyms).
 */
export function monogram(name: string): string {
	const words = name
		.replace(/([a-z0-9])([A-Z])/g, "$1 $2")
		.split(/[^\p{L}\p{N}]+/u)
		.filter(Boolean);
	if (!words.length) return "?";
	return words
		.slice(0, 2)
		.map((w) => [...w][0].toUpperCase())
		.join("");
}

/**
 * Pick what to show. `detected` is the repo logo URL (null when none was found or detection
 * has not finished). A user-chosen image that failed to load falls through to detection, then
 * the monogram, so a deleted logo never leaves a blank slot. `imageSrc` resolves the chosen
 * image's URL (null while loading / missing).
 */
export function resolveIcon(
	name: string,
	identity: Identity | null | undefined,
	detected: string | null,
	imageSrc: (path: string) => string | null = () => null,
): ResolvedIcon {
	const c = identity?.icon;
	if (c?.kind === "emoji" && c.value.trim()) return { kind: "emoji", value: c.value };
	if (c?.kind === "lucide" && c.value) return { kind: "lucide", value: c.value };
	if (c?.kind === "image") {
		const src = imageSrc(c.value);
		if (src) return { kind: "image", src, auto: false };
	}
	if (detected) return { kind: "image", src: detected, auto: true };
	return { kind: "monogram", text: monogram(name) };
}

/** Accent hue of a space: the user's choice when valid, else the (spread) derived one. */
export function hueFor(id: string, identity: Identity | null | undefined, spread?: number): number {
	const h = identity?.hue;
	return h !== undefined && Number.isInteger(h) && h >= 0 && h < 360 ? h : (spread ?? hueOf(id));
}

/** Hues offered as swatches; the first slot ("auto") clears the choice. */
export const SWATCH_HUES: readonly number[] = [25, 55, 95, 145, 175, 205, 240, 275, 310, 350];

/** Substring search over icon names, prefix matches first, capped for rendering. */
export function searchIcons(names: readonly string[], query: string, limit = 120): string[] {
	const q = query.trim().toLowerCase().replace(/\s+/g, "-");
	if (!q) return names.slice(0, limit);
	const starts: string[] = [];
	const has: string[] = [];
	for (const n of names) {
		if (n.startsWith(q)) starts.push(n);
		else if (n.includes(q)) has.push(n);
		if (starts.length >= limit) break;
	}
	return [...starts, ...has].slice(0, limit);
}

/**
 * Icon for the tray popover, which has no host access and no Lucide table: emoji and monogram
 * pass through, an image is sent as its data URL, a Lucide choice shows the monogram.
 */
export function trayIconOf(icon: ResolvedIcon, name: string, dataUrl: string | null) {
	if (icon.kind === "emoji") return { kind: "emoji", text: icon.value } as const;
	if (icon.kind === "image" && dataUrl) return { kind: "image", src: dataUrl } as const;
	return { kind: "monogram", text: icon.kind === "monogram" ? icon.text : monogram(name) } as const;
}
