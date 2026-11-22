import { defineCollection } from "astro:content";
import { z } from "astro/zod";
import { glob } from "astro/loaders";
import { ISSUE_URL, MACHINE, PENDING, SOURCE_FILE, SOURCE_SHA } from "./lib/doc-translation";

// Single source of truth: the repository's docs/*.md. Nothing is copied; the
// loader reads the files in place. README.md is the GitHub index, not a page.
const docs = defineCollection({
	loader: glob({ pattern: ["*.md", "!README.md"], base: "../docs" }),
	schema: z.object({
		title: z.string().optional(),
		description: z.string().optional(),
	}),
});

// Portuguese (pt-BR) translations: docs/pt-BR/<slug>.md mirrors docs/<slug>.md. The English pattern above is not recursive, so
// these never leak into `docs`. README.md is the GitHub index of the translations, not a page. Front matter is `title`, `source`, `source_sha` and optionally `translation: machine` or `translation: pending` with `issue` (see lib/doc-translation.ts).
const docsPtBr = defineCollection({
	loader: glob({ pattern: ["*.md", "!README.md"], base: "../docs/pt-BR" }),
	schema: z
		.object({
			title: z.string().min(1),
			source: z.string().regex(SOURCE_FILE),
			source_sha: z.string().regex(SOURCE_SHA),
			translation: z.enum([MACHINE, PENDING]).optional(),
			issue: z.string().regex(ISSUE_URL).optional(),
		})
		.strict(),
});

export const collections = { docs, docsPtBr };
