// Compiles site/messages into src/lib/paraglide (gitignored) with the same options as the Vite plugin.
import { compile } from "@inlang/paraglide-js";
import { paraglideOptions } from "../paraglide.options.mjs";

await compile({ ...paraglideOptions, cwd: new URL("..", import.meta.url).pathname });
