// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * `bun test` preload for component tests: a DOM (happy-dom) and a loader that compiles
 * `.svelte` / `.svelte.ts` with the client compiler, so tests mount real components with
 * real runes. `#lib/*` resolves through package.json `imports` like in the app.
 */
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { plugin } from "bun";
import { compile, compileModule } from "svelte/compiler";

GlobalRegistrator.register();
// The suite is written against the macOS app (⌘ shortcuts, glyph labels); happy-dom reports an X11/Win32 platform.
// Platform-specific code takes the platform as a parameter and its Windows/Linux branches are tested explicitly.
Object.defineProperty(navigator, "platform", { value: "MacIntel", configurable: true });

plugin({
	name: "svelte",
	setup(build) {
		build.onLoad({ filter: /\.svelte$/ }, async ({ path }) => {
			// Svelte 5 strips `lang="ts"` types itself, in the script and in the markup.
			const out = compile(await Bun.file(path).text(), { filename: path, generate: "client", dev: true });
			return { contents: out.js.code, loader: "js" };
		});
		build.onLoad({ filter: /\.svelte\.ts$/ }, async ({ path }) => {
			const js = new Bun.Transpiler({ loader: "ts" }).transformSync(await Bun.file(path).text());
			const out = compileModule(js, { filename: path, generate: "client", dev: true });
			return { contents: out.js.code, loader: "js" };
		});
	},
});
