// `bun studio publish <film> [--release vX.Y.Z]`: copy rendered outputs into site/public/media under their stable
// names, record content hashes in site/public/media/manifest.json, write film-chapters.json for the site player,
// and optionally attach the videos to a GitHub release.

import { copyFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { run } from "./lib/exec.ts";
import { loadFilm, parseArgs } from "./lib/films.ts";
import { OUT, SITE_MEDIA, StudioError } from "./lib/paths.ts";
import type { RenderReport } from "./render.ts";

interface MediaManifest {
	version: 1;
	files: Record<string, { sha256: string; bytes: number; film: string; buildHash: string }>;
}

/** Thumbnail tiles of the scrub sprite (see render.ts); the site player needs them to address the sprite. */
const THUMB = { step: 2, w: 160, h: 90, cols: 10 };

export async function main(argv: string[]): Promise<void> {
	const { pos, flags } = parseArgs(argv);
	const film = await loadFilm(pos[0] ?? "");
	const out = join(OUT, film.id);
	const manifestPath = join(SITE_MEDIA, "manifest.json");
	const manifest: MediaManifest = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, "utf8")) : { version: 1, files: {} };
	const uploads: string[] = [];

	for (const cut of film.cuts) {
		const reportPath = join(out, `${cut.out}.report.json`);
		if (!existsSync(reportPath)) throw new StudioError(`${cut.out} is not rendered`, `bun studio render ${film.id} --aspect ${cut.aspect}`);
		const report: RenderReport = JSON.parse(readFileSync(reportPath, "utf8"));
		if (report.draft) throw new StudioError(`${cut.out} is a draft render`, `bun studio render ${film.id} --aspect ${cut.aspect}   (without --draft)`);
		for (const [name, info] of Object.entries(report.files)) {
			copyFileSync(join(out, name), join(SITE_MEDIA, name));
			manifest.files[name] = { ...info, film: film.id, buildHash: report.buildHash };
			if (name.endsWith(".mp4") || name.endsWith(".webm")) uploads.push(join(out, name));
		}
		if (cut.chapters) {
			const chapters = { film: film.id, duration: +report.duration.toFixed(2), thumb: THUMB, chapters: cut.chapters };
			writeFileSync(join(SITE_MEDIA, `${cut.out}-chapters.json`), `${JSON.stringify(chapters, null, "\t")}\n`);
			console.log(`  chapters -> site/public/media/${cut.out}-chapters.json (${cut.chapters.length} chapters, ${chapters.duration} s)`);
		}
	}
	const sorted = Object.fromEntries(Object.entries(manifest.files).sort(([a], [b]) => a.localeCompare(b)));
	writeFileSync(manifestPath, `${JSON.stringify({ version: 1, files: sorted }, null, "\t")}\n`);
	console.log(`✓ published ${film.id} to site/public/media (hashes in manifest.json)`);

	const tag = flags.release;
	if (typeof tag === "string") {
		await run(["gh", "release", "upload", tag, ...uploads, "--clobber"], { live: true });
		console.log(`✓ attached ${uploads.length} files to ${tag}`);
	}
}
