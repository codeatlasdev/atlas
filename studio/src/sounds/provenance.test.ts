import { describe, expect, test } from "bun:test";
import { EARCONS } from "./spec.ts";
import { provenanceRow, readme } from "./provenance.ts";

// Incident: an earcon .m4a was committed with no README entry. The README was only written when ONE `pick` run finished
// all categories, so picking a single new earcon never updated it. The README must be built from every shipped file.
describe("provenance README", () => {
	const row = (id: keyof typeof EARCONS) =>
		provenanceRow(id, { ms: 1740, rmsDb: -22, peakDb: -7.3, centroidHz: 851, bytes: 12298, take: 7, key: "abcdef0123456789" });

	test("every earcon with a shipped file has a row, in the spec's order, none dropped", () => {
		const ids = Object.keys(EARCONS) as (keyof typeof EARCONS)[];
		const md = readme(ids.map(row), "2026-10-12");
		for (const id of ids) expect(md).toContain(`| \`${id}\` |`);
		const order = ids.map((id) => md.indexOf(`| \`${id}\` |`));
		expect(order).toEqual([...order].sort((a, b) => a - b));
	});

	test("a row carries the length, take and key, so a shipped file is reproducible", () => {
		const md = row("boot");
		expect(md).toContain("1740 ms");
		expect(md).toContain("take 7");
		expect(md).toContain("abcdef0123456789");
	});

	test("states the provider, endpoint, model, licence terms, date and that nothing came from another library", () => {
		const md = readme([row("boot")], "2026-10-12");
		expect(md).toContain("ElevenLabs");
		expect(md).toContain("/v1/sound-generation");
		expect(md).toContain("eleven_text_to_sound_v2");
		expect(md).toContain("2026-10-12");
		expect(md).toContain("terms-of-use");
		expect(md).toContain("Nothing here was taken from another sound library");
	});

	test("lists every prompt exactly as sent (the boot signature's included)", () => {
		const md = readme([row("boot")], "2026-10-12");
		expect(md).toContain("- `boot`: ");
	});

	test("documents the boot signature's post-processing difference (40 dB tail cut), not the feedback ones", () => {
		const md = readme([row("boot")], "2026-10-12");
		expect(md).toMatch(/40 dB/);
		expect(md).toContain("`boot` signature");
	});
});
