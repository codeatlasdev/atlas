// Generates site/.i18n from the one source tree (i18n/messages), then compiles it into src/lib/paraglide (gitignored)
// with the same options as the Vite plugin.
import { compile } from "@inlang/paraglide-js";
import { generateMessages, paraglideOptions } from "../paraglide.options.mjs";

generateMessages();

await compile({ ...paraglideOptions, cwd: new URL("..", import.meta.url).pathname });
