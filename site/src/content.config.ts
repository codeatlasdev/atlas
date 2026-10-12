import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

// Single source of truth: the repository's docs/*.md. Nothing is copied; the
// loader reads the files in place. README.md is the GitHub index, not a page.
const docs = defineCollection({
	loader: glob({ pattern: ["*.md", "!README.md"], base: "../docs" }),
	schema: z.object({
		title: z.string().optional(),
		description: z.string().optional(),
	}),
});

export const collections = { docs };
