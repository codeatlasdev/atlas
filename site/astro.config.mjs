import { defineConfig } from "astro/config";
import { unified } from "@astrojs/markdown-remark";
import svelte from "@astrojs/svelte";
import sitemap from "@astrojs/sitemap";
import pagefind from "astro-pagefind";
import rehypeSlug from "rehype-slug";
import rehypeAutolinkHeadings from "rehype-autolink-headings";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { inSitemap, withXDefault } from "./src/lib/i18n.ts";
import { paraglideOptions } from "./paraglide.options.mjs";
import remarkDocsLinks from "./src/lib/remark-docs-links.mjs";
import remarkDocsTitle from "./src/lib/remark-docs-title.mjs";

// Docs with a pt-BR translation (docs/pt-BR/<slug>.md). Only these pt-BR pages are real translations: the rest of /pt-br/docs/ shows the
// English body (see src/pages/pt-br/docs/[...slug].astro) and stays out of the sitemap.
const translatedDocs = new Set(
	(existsSync(new URL("../docs/pt-BR/", import.meta.url)) ? readdirSync(new URL("../docs/pt-BR/", import.meta.url)) : []).filter((f) => f.endsWith(".md")).map((f) => f.slice(0, -3)),
);

const atlasNight = JSON.parse(readFileSync(new URL("./src/lib/atlas-shiki-theme.json", import.meta.url), "utf8"));

export default defineConfig({
	site: "https://atlas.codeatlas.com.br",
	output: "static",
	trailingSlash: "always",
	// English at /, Portuguese at /pt-br/ (URL segment `pt-br`, tag pt-BR). Pages without a pt-BR twin fall back to English with a REDIRECT
	// (Astro's default fallbackType): a page that is not translated is not duplicated under /pt-br/, so there is no second copy to
	// canonicalize, no banner to maintain and no way to land on English text believing it was translated. The docs are the exception: they
	// are generated for every slug (a translation or an English body with a banner), see src/pages/pt-br/docs/[...slug].astro.
	i18n: {
		locales: ["en", { path: "pt-br", codes: ["pt-BR", "pt"] }],
		defaultLocale: "en",
		routing: { prefixDefaultLocale: false },
		fallback: { "pt-br": "en" },
	},
	integrations: [
		svelte(),
		sitemap({
			// hreflang alternates inside the sitemap come from this option; fallback pages are filtered out so no alternate points at a copy.
			i18n: { defaultLocale: "en", locales: { en: "en", "pt-br": "pt-BR" } },
			filter: (page) => !page.endsWith("/docs/") && inSitemap(new URL(page).pathname, translatedDocs),
			serialize: (item) => ({ ...item, links: withXDefault(item.links) }),
		}),
		pagefind(),
	],
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
	vite: { plugins: [paraglideVitePlugin(paraglideOptions)] },
});
