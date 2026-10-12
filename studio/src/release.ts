// `bun studio release <version|unreleased> [--voice] [--aspect 16:9|9:16] [--draft] [--offline]`: a short "What's new"
// video from a CHANGELOG.md section: an intro card, one card per highlight (with the best-matching app clip from
// studio/clips/library.json when one fits), and an outro. Output: studio/out/release-<version>/ in 16:9 and 9:16.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { hasAsset } from "./lib/assets.ts";
import { parseArgs, pickCuts } from "./lib/films.ts";
import { REPO, STUDIO, StudioError } from "./lib/paths.ts";
import type { AppClipScene, AspectId, Cut, Film, NarrationLine, Scene, TitleScene } from "./lib/types.ts";
import { renderCut } from "./render.ts";

export interface Highlight {
	/** short headline (bold lead of the bullet, or the text before the first colon, or the first clause) */
	title: string;
	/** one-sentence summary, markdown stripped */
	body: string;
}

interface LibraryClip {
	id: string;
	clip: string;
	from: number;
	seconds: number;
	tags: string[];
}

interface Library {
	clips: LibraryClip[];
	crop: { x: number; y: number; w: number; h: number };
}

const strip = (s: string) =>
	s
		.replace(/\*\*([^*]+)\*\*/g, "$1")
		.replace(/`([^`]+)`/g, "$1")
		.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
		.replace(/_([^_]+)_/g, "$1")
		.replace(/\s+/g, " ")
		.trim();

/** The body of a version section of CHANGELOG.md: `Unreleased`, or a version like `1.1.0` (heading `## [1.1.0] — date`). */
export function changelogSection(md: string, version: string): { heading: string; body: string } {
	const want = /^unreleased$/i.test(version) ? "Unreleased" : version.replace(/^v/, "");
	const lines = md.split("\n");
	const start = lines.findIndex((l) => l.startsWith("## [") && l.slice(4).startsWith(want));
	if (start < 0) throw new StudioError(`CHANGELOG.md has no section [${want}]`, `sections: ${lines.filter((l) => l.startsWith("## [")).map((l) => l.slice(3, l.indexOf("]") + 1)).join(" ")}`);
	const end = lines.findIndex((l, i) => i > start && l.startsWith("## ["));
	return { heading: lines[start]!, body: lines.slice(start + 1, end < 0 ? undefined : end).join("\n") };
}

/** First sentence of a bullet, without markdown, at most 120 chars. */
function firstSentence(text: string): string {
	const t = strip(text);
	const m = /^(.+?[.!?])(\s|$)/.exec(t);
	const s = m?.[1] ?? t;
	return s.length > 120 ? `${s.slice(0, 117).trimEnd()}…` : s;
}

/**
 * Highlights of a section. When the section has a `### Highlights` block those bullets win; otherwise the bullets of
 * `### Added` then `### Changed`. A bullet's title is its bold lead (`**Updates, shown properly.**`), else the text
 * before the first colon, else its first clause.
 */
export function highlights(section: string, max = 5): Highlight[] {
	const blocks = new Map<string, string[]>();
	let cur = "";
	for (const line of section.split("\n")) {
		const h = /^###\s+(.+)$/.exec(line);
		if (h) {
			cur = h[1]!.trim();
			blocks.set(cur, []);
		} else if (/^[-*]\s+/.test(line) && cur) blocks.get(cur)!.push(line.replace(/^[-*]\s+/, ""));
	}
	const order = ["Highlights", "Added", "Changed"];
	const pool = order.flatMap((k) => blocks.get(k) ?? []);
	const picked = (blocks.get("Highlights")?.length ? (blocks.get("Highlights") ?? []) : pool).slice(0, max);
	return picked.map((b) => {
		const bold = /^\*\*([^*]+?)\.?\*\*\s*(.*)$/.exec(b);
		if (bold) return { title: strip(bold[1]!), body: firstSentence(bold[2] || bold[1]!) };
		const colon = /^([^:]{3,48}):\s+(.*)$/.exec(strip(b));
		if (colon) return { title: colon[1]!, body: firstSentence(colon[2]!) };
		const clause = strip(b).split(/[,;.]/)[0] ?? "";
		return { title: clause.length > 48 ? `${clause.slice(0, 45).trimEnd()}…` : clause, body: firstSentence(b) };
	});
}

const words = (s: string) => s.toLowerCase().match(/[a-z]{3,}/g) ?? [];

/** The library clip whose tags overlap the highlight text most (ties: library order); none when nothing overlaps or the recording is not cached. */
export function matchClip(h: Highlight, lib: Library, used: ReadonlySet<string> = new Set()): LibraryClip | undefined {
	const w = new Set(words(`${h.title} ${h.body}`));
	let best: { clip: LibraryClip; score: number } | undefined;
	for (const clip of lib.clips) {
		if (used.has(clip.id) || !hasAsset(clip.clip)) continue;
		const score = clip.tags.filter((t) => w.has(t)).length;
		if (score > 0 && (!best || score > best.score)) best = { clip, score };
	}
	return best?.clip;
}

function scenesFor(aspect: AspectId, version: string, hs: Highlight[], lib: Library): { scenes: Scene[]; lines: NarrationLine[] } {
	const wide = aspect === "16:9";
	const bgs = wide ? ["a", "b", "c"] : ["v"];
	const scenes: Scene[] = [];
	const lines: NarrationLine[] = [];
	let t = 0;
	// each recording appears once per video: a second highlight about the same area gets a text card
	const used = new Set<string>();
	const intro: TitleScene = {
		id: "intro",
		type: "title",
		duration: 3.5,
		bg: bgs[0],
		zoom: 0.0005,
		transitionIn: { type: "cut", duration: 0 },
		mark: wide ? { size: 220, y: 230, delay: 0.2, fade: 0.8 } : { size: 260, y: 520, delay: 0.2, fade: 0.8 },
		text: { title: `Atlas ${version}`, subtitle: "What's new", y: wide ? 560 : 840, titleSize: wide ? 130 : 130, delay: 0.9, fade: 0.6 },
	};
	scenes.push(intro);
	lines.push({ id: "intro", at: 0.5, text: `Atlas ${version}. What's new.` });
	t += intro.duration;
	for (const [i, h] of hs.entries()) {
		const clip = matchClip(h, lib, used);
		if (clip) used.add(clip.id);
		const id = `h${i + 1}`;
		const title = { title: h.title, subtitle: h.body, y: wide ? 60 : 200, titleSize: wide ? 68 : 72, subtitleSize: wide ? 30 : 34, maxWidth: wide ? 1500 : 940 };
		if (clip) {
			const app: AppClipScene = {
				id,
				type: "app",
				clip: clip.clip,
				from: clip.from,
				duration: 6,
				bg: bgs[(i + 1) % bgs.length],
				crop: lib.crop,
				box: wide ? { x: "center", y: 290, w: 1160, h: 726 } : { x: "center", y: 720, w: 1000 },
				push: { from: 1, to: 1.05 },
				overlay: title,
			};
			scenes.push(app);
		} else {
			scenes.push({ id, type: "title", duration: 4, bg: bgs[(i + 1) % bgs.length], zoom: 0.0005, text: { ...title, y: wide ? 400 : 760, titleSize: wide ? 96 : 92, subtitleSize: wide ? 38 : 40, maxWidth: wide ? 1400 : 900, delay: 0.2, fade: 0.5 } });
		}
		lines.push({ id, at: t + 0.4, text: `${h.title}. ${h.body}` });
		t += scenes.at(-1)!.duration;
	}
	scenes.push({
		id: "outro",
		type: "title",
		duration: 4,
		bg: bgs[0],
		zoom: 0.0004,
		transitionIn: { type: "dip", duration: 0.5 },
		fadeOut: 0.6,
		mark: wide ? { size: 200, y: 250, delay: 0, fade: 0.01 } : { size: 240, y: 560, delay: 0, fade: 0.01 },
		text: { title: `Atlas ${version}`, subtitle: "Available now", url: "atlas.codeatlas.com.br", y: wide ? 560 : 860, titleSize: 110, delay: 0.3, fade: 0.6 },
	});
	return { scenes, lines };
}

/** The Film for a release video: not stored, derived from the changelog every time so it can never drift from it. */
export function releaseFilm(version: string, hs: Highlight[], lib: Library, voice: boolean): Film {
	const cuts: Cut[] = (["16:9", "9:16"] as const).map((aspect) => {
		const { scenes, lines } = scenesFor(aspect, version, hs, lib);
		const cut: Cut = {
			aspect,
			out: `release-${version}${aspect === "9:16" ? "-vertical" : ""}`,
			scenes,
			captions: { vtt: aspect === "16:9", burned: aspect === "9:16" },
		};
		if (voice) {
			cut.voice = { id: "cjVigY5qzO86Huf0OWal", tag: "[confident, warm, modern product launch narrator]", stability: 0.5, similarity: 0.7, model: "eleven_v4", lufs: -14, lines };
		}
		return cut;
	});
	return {
		id: `release-${version}`,
		title: `Atlas ${version}: what's new`,
		fps: 30,
		crf: 26,
		loudness: { target: -14, tolerance: 0.7 },
		music: {
			bed: "music:bed",
			bedLufs: voice ? -20 : -14,
			fadeIn: 1.5,
			fadeOut: 3,
			duck: { threshold: 0.03, ratio: 2.5, attack: 80, release: 600 },
			source: "ElevenLabs Music (music_v1), instrumental, generated for the Atlas 1.0 film",
			license: "Generated music, used under the ElevenLabs plan terms of the account that generated it.",
		},
		cuts,
	};
}

export async function main(argv: string[]): Promise<void> {
	const { pos, flags } = parseArgs(argv);
	const version = pos[0];
	if (!version) throw new StudioError("version required", "bun studio release <1.1.0|unreleased> [--voice] [--aspect 16:9|9:16] [--draft]");
	const md = readFileSync(join(REPO, "CHANGELOG.md"), "utf8");
	const { heading, body } = changelogSection(md, version);
	const hs = highlights(body, typeof flags.max === "string" ? Number(flags.max) : 4);
	if (hs.length === 0) throw new StudioError(`${heading.trim()} has no bullets under Highlights/Added/Changed`);
	const libPath = join(STUDIO, "clips/library.json");
	if (!existsSync(libPath)) throw new StudioError("studio/clips/library.json is missing");
	const lib: Library = JSON.parse(readFileSync(libPath, "utf8"));
	// "unreleased" has no number yet: the next minor, or the name given with --as
	const label = typeof flags.as === "string" ? flags.as : /^unreleased$/i.test(version) ? "next" : version.replace(/^v/, "");
	const film = releaseFilm(label, hs, lib, flags.voice === true);
	const listed = new Set<string>();
	for (const h of hs) {
		const c = matchClip(h, lib, listed);
		if (c) listed.add(c.id);
		console.log(`  • ${h.title}  [${c?.id ?? "text card"}]`);
	}
	for (const cut of pickCuts(film, flags.aspect)) {
		const r = await renderCut(film, cut, { draft: flags.draft === true, offline: flags.offline === true, minimal: flags.draft === true });
		console.log(`✓ ${cut.out}  ${r.duration.toFixed(1)} s  ${r.width}x${r.height}  ${r.lufs.toFixed(1)} LUFS`);
	}
}
