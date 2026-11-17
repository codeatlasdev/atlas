// Browser side of Paraglide: an island or script that calls `m.*()` reads the locale from the page's own <html lang> (Base.astro sets
// it from Astro's routing). Import this module before the first `m.*()` call in client code. Nothing writes the locale in the browser:
// switching language is a link to the other page.
import { defineCustomClientStrategy } from "./paraglide/runtime.js";
import { localeOfAstro } from "./i18n";

defineCustomClientStrategy("custom-htmlLang", {
	getLocale: () => localeOfAstro(document.documentElement.lang),
	setLocale: () => {},
});
