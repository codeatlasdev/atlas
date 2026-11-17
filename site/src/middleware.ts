// Static site generation: there is no request, so Paraglide is told which locale each page renders in. This runs while Astro
// prerenders every page (build.concurrency is 1, so pages render one at a time and the global variable cannot leak between them).
// Do not use paraglideMiddleware() here: it is for SSR (Paraglide docs, "Static Site Generation").
import { defineMiddleware } from "astro:middleware";
import { setLocale } from "./lib/paraglide/runtime.js";
import { localeOfAstro } from "./lib/i18n";

export const onRequest = defineMiddleware((context, next) => {
	setLocale(localeOfAstro(context.currentLocale));
	return next();
});
