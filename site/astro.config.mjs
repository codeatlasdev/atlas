import { defineConfig } from "astro/config";
import { unified } from "@astrojs/markdown-remark";
import svelte from "@astrojs/svelte";
import sitemap from "@astrojs/sitemap";
import pagefind from "astro-pagefind";
import rehypeSlug from "rehype-slug";
import rehypeAutolinkHeadings from "rehype-autolink-headings";
import { readFileSync } from "node:fs";
import remarkDocsLinks from "./src/lib/remark-docs-links.mjs";
import remarkDocsTitle from "./src/lib/remark-docs-title.mjs";

const atlasNight = JSON.parse(readFileSync(new URL("./src/lib/atlas-shiki-theme.json", import.meta.url), "utf8"));

export default defineConfig({
	site: "https://atlas.codeatlas.com.br",
	output: "static",
	trailingSlash: "always",
	integrations: [svelte(), sitemap({ filter: (page) => !page.endsWith("/docs/") }), pagefind()],
	markdown: {
		shikiConfig: { theme: atlasNight, wrap: false },
		processor: unified({
			remarkPlugins: [remarkDocsLinks, remarkDocsTitle],
			rehypePlugins: [
				rehypeSlug,
				[
					rehypeAutolinkHeadings,
					{ behavior: "append", properties: { class: "anchor", ariaLabel: "Link to this section" }, content: { type: "text", value: "#" } },
				],
			],
		}),
	},
	// First paint must not wait on three stylesheet round trips: inline all CSS.
	build: { inlineStylesheets: "always" },
	prefetch: { prefetchAll: false },
	image: { layout: "constrained" },
});
