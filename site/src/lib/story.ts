// Release stories: the hand-written, illustrated post of a version (src/content/releases/<locale>/<id>.yaml). Pure (no `astro:` imports) so
// the content collection, the page and bun test share one shape and one set of rules.
import { z } from "astro/zod";

const KEBAB = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

const art = z
	.object({
		/** basename of src/components/changelog/art/<type>.astro */
		type: z.string().regex(KEBAB),
		alt: z.string().min(1),
		caption: z.string().min(1).optional(),
		/** every text drawn inside the illustration; the art file documents its keys */
		labels: z.record(z.string(), z.string()).default({}),
	})
	.strict();

/** A real still or clip that already exists in public/media (`shot-*.jpg`, `loop-*` with its `-poster.jpg`). */
const media = z.union([z.object({ still: z.string().regex(/^[\w-]+\.(?:jpg|png)$/), alt: z.string().min(1) }).strict(), z.object({ clip: z.string().regex(KEBAB), alt: z.string().min(1) }).strict()]);

const highlight = z
	.object({
		id: z.string().regex(KEBAB),
		title: z.string().min(1).max(70),
		body: z.string().min(1),
		art: art.optional(),
		media: media.optional(),
	})
	.strict()
	.refine((h) => !(h.art && h.media), { message: "a highlight has art or media, not both" });

export const storySchema = z
	.object({
		version: z.string().regex(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/),
		locale: z.enum(["en", "pt-BR"]),
		title: z.string().min(1).max(80),
		intro: z.string().min(1),
		highlights: z.array(highlight).min(3).max(6),
		also: z.array(z.string().min(1)).min(1).max(10),
	})
	.strict();

export type Story = z.infer<typeof storySchema>;
export type Highlight = Story["highlights"][number];

/** The anchor id of a version, "v1-5-0-alpha-27" (same rule as functions/_lib/changelog.ts `versionId`). */
export const storyId = (version: string) => `v${version.replace(/[^0-9A-Za-z]+/g, "-").replace(/^-|-$/g, "")}`;

/** What is wrong between the two languages of one story: the translation must tell the same story (same highlights, order, art and labels). */
export function twinIssues(en: Story, pt: Story): string[] {
	const out: string[] = [];
	if (en.version !== pt.version) out.push(`version differs: ${en.version} / ${pt.version}`);
	if (en.highlights.map((h) => h.id).join() !== pt.highlights.map((h) => h.id).join()) out.push("highlight ids or order differ");
	else
		en.highlights.forEach((h, i) => {
			const p = pt.highlights[i];
			if (h.art?.type !== p.art?.type) out.push(`${h.id}: art type differs`);
			if ((h.media === undefined) !== (p.media === undefined)) out.push(`${h.id}: media differs`);
			const a = Object.keys(h.art?.labels ?? {}).sort().join();
			const b = Object.keys(p.art?.labels ?? {}).sort().join();
			if (a !== b) out.push(`${h.id}: label keys differ (${a} / ${b})`);
		});
	return out;
}
