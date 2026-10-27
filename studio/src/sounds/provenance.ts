// The README that ships beside the earcons, built from a record of every shipped file (not from one `pick` run), so
// picking a single earcon can never leave the others, or itself, undocumented.

import { EARCONS, FAMILY, MODEL, promptFor, type EarconId } from "./spec.ts";

export const ENDPOINT = "POST https://api.elevenlabs.io/v1/sound-generation";
/** Loudness target of the finished files, restated in the README. */
export const TARGET = { rmsDb: -22, peakCeilingDb: -3 } as const;

/** What `pick` measured on the finished file; persisted so the README can be rebuilt for every shipped earcon. */
export interface Shipped {
	ms: number;
	rmsDb: number;
	peakDb: number;
	centroidHz: number;
	bytes: number;
	take: number;
	/** Digest of the prompt the take was generated from (spec.ts `takeName`). */
	key: string;
}

export function provenanceRow(id: EarconId, s: Shipped): string {
	return `| \`${id}\` | ${EARCONS[id].meaning} | ${s.ms} ms | ${s.rmsDb.toFixed(1)} | ${s.peakDb.toFixed(1)} | ${Math.round(s.centroidHz)} | ${s.bytes} | take ${s.take}, ${s.key} |`;
}

/** The full README for `rows` (already in display order) generated on `date`. */
export function readme(rows: string[], date: string): string {
	const prompts = (Object.keys(EARCONS) as EarconId[]).map((id) => `- \`${id}\`: ${promptFor(EARCONS[id])}`).join("\n");
	return `# Atlas earcons: provenance

Generated ${date} with the ElevenLabs Sound Effects (text-to-sound) API: \`${ENDPOINT}\`, model \`${MODEL}\`,
\`prompt_influence\` 0.7, then trimmed, gain-matched and encoded by \`bun studio sounds pick\` (studio/src/sounds).
Nothing here was taken from another sound library or the internet.

**License.** The files are output of the ElevenLabs API produced under the maintainers' paid account. Per ElevenLabs'
terms (https://elevenlabs.io/terms-of-use) generated output may be used commercially on paid plans; this was
generated while the account was on the plan the maintainers hold, and the terms at that date apply. If the account
plan changes, re-check the terms before redistributing the files outside Atlas.

Post-processing of the feedback earcons: leading/trailing silence cut at -45 dBFS (3 ms kept before the attack), the tail
cut where the note has decayed 30 dB below its peak, gain set so the RMS of the finished file is ${TARGET.rmsDb} dBFS with
the peak under ${TARGET.peakCeilingDb} dBFS, 5 ms fade in and out (30 ms, or a quarter of the cap, when a take is cut at
its length cap), mono, 48 kHz, AAC 48 kb/s in an .m4a container (WebKit decodes it natively; only the .m4a files ship).

The \`boot\` signature differs in one way: its tail is cut at 40 dB of decay (a bell rings on quietly and that is part of it).

| id | meaning | length | RMS dBFS | peak dBFS | centroid Hz | .m4a bytes | take |
|---|---|---|---|---|---|---|---|
${rows.join("\n")}

## Prompts

Every prompt carries the same timbre sentence ("${FAMILY}") so the set reads as one brand.

${prompts}

## Regenerating

\`bun studio sounds generate [id…] --variants 4\` then \`bun studio sounds pick [id…]\`. The ElevenLabs key is read from the
environment or \`ATLAS_STUDIO_ENV\` at run time only. Takes are cached (gitignored) in studio/.cache/sounds. \`pick\` records
what it shipped in provenance.json beside this file and rebuilds this README from it for every earcon on disk.
`;
}
