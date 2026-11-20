// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { describe, expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { storyId, storySchema, twinIssues, type Story } from "./story";

// Every release story of the repo: both languages exist, parse, tell the same story and point at real illustrations and media.
const ROOT = new URL("../content/releases/", import.meta.url);
const ART = new URL("../components/changelog/art/", import.meta.url);
const MEDIA = new URL("../../public/media/", import.meta.url);
const load = (locale: string, file: string): Story => storySchema.parse(Bun.YAML.parse(readFileSync(new URL(`${locale}/${file}`, ROOT), "utf8")));
const files = readdirSync(new URL("en/", ROOT)).filter((f) => f.endsWith(".yaml"));

describe("release stories", () => {
	test("there are stories, and every English one has a Portuguese twin (and the reverse)", () => {
		expect(files.length).toBeGreaterThan(0);
		expect(readdirSync(new URL("pt-BR/", ROOT)).sort()).toEqual([...files].sort());
	});
	for (const file of files) {
		const id = file.replace(/\.yaml$/, "");
		test(`${id}: file name is the version's anchor id, locale matches the folder, twins agree`, () => {
			const en = load("en", file);
			const pt = load("pt-BR", file);
			expect(storyId(en.version)).toBe(id);
			expect([en.locale, pt.locale]).toEqual(["en", "pt-BR"]);
			expect(twinIssues(en, pt)).toEqual([]);
		});
		test(`${id}: every illustration exists, labels are filled in, no em dashes`, () => {
			for (const locale of ["en", "pt-BR"]) {
				const s = load(locale, file);
				for (const h of s.highlights) {
					if (h.art) {
						expect(existsSync(new URL(`${h.art.type}.astro`, ART))).toBe(true);
						expect(Object.values(h.art.labels).every((v) => v.trim().length > 0)).toBe(true);
					}
					if (h.media && "still" in h.media) expect(existsSync(new URL(h.media.still, MEDIA))).toBe(true);
				}
				expect(JSON.stringify(s)).not.toContain("\u2014");
			}
		});
	}
	test("twinIssues names what differs", () => {
		const en = load("en", files[0]);
		const broken: Story = { ...en, highlights: [...en.highlights].reverse() };
		expect(twinIssues(en, broken).length).toBeGreaterThan(0);
	});
	test("a highlight has art or media, not both", () => {
		const base = load("en", files[0]);
		const bad = { ...base, highlights: [{ ...base.highlights[0], media: { still: "shot-agents.jpg", alt: "x" } }, ...base.highlights.slice(1)] };
		expect(storySchema.safeParse(bad).success).toBe(false);
	});
});
