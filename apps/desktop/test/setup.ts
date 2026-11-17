// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * `bun test` preload for component tests: a DOM (happy-dom) and a loader that compiles
 * `.svelte` / `.svelte.ts` with the client compiler, so tests mount real components with
 * real runes. `#lib/*` resolves through package.json `imports` like in the app.
 */
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { plugin } from "bun";
import { compile, compileModule } from "svelte/compiler";
import { compileMessages } from "../scripts/paraglide.ts";

// Dates in assertions must not depend on the machine running them (`bun run test` also sets TZ and LC_ALL; this keeps a
// bare `bun test <file>` the same).
process.env.TZ = "UTC";

// Without `--conditions browser` Bun resolves Svelte's server build, where `mount` throws: every component test fails
// with an unrelated-looking error. Stop with the reason instead.
if (!Bun.resolveSync("svelte", import.meta.dir).includes("index-client")) {
	throw new Error(
		"Svelte resolved to its server build: run `bun run test` (it passes `--conditions browser`).",
	);
}

GlobalRegistrator.register();
// The suite is written against the macOS app (⌘ shortcuts, glyph labels); happy-dom reports an X11/Win32 platform.
// Platform-specific code takes the platform as a parameter and its Windows/Linux branches are tested explicitly.
Object.defineProperty(navigator, "platform", { value: "MacIntel", configurable: true });

// The kit's layers are `popover` elements (and `<dialog>`s): happy-dom has no Popover API, so this stands in for the
// part the kit calls (`showPopover`, `hidePopover`, `:popover-open`). A real browser provides all of it.
const open = new WeakSet();
HTMLElement.prototype.showPopover ??= function () {
	open.add(this);
};
HTMLElement.prototype.hidePopover ??= function () {
	open.delete(this);
};
const matches = Element.prototype.matches;
Element.prototype.matches = function (selector) {
	return selector === ":popover-open" ? open.has(this) : matches.call(this, selector);
};

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
		// The kit ships its stores (`*.svelte.js`, runes) uncompiled: they must go through the same compiler.
		build.onLoad({ filter: /\.svelte\.js$/ }, async ({ path }) => {
			const out = compileModule(await Bun.file(path).text(), {
				filename: path,
				generate: "client",
				dev: true,
			});
			return { contents: out.js.code, loader: "js" };
		});
	},
});

// The display language: compile the catalogs when they changed (the same compiler and options as Vite), then register
// the Svelte-state locale strategy so every `m.*()` in a test reads it. Tests run in the base language ("en");
// a test that needs another one sets it with `inLocale` (test/locale.ts) and gets "en" back.
await compileMessages(true);
await import("#lib/i18n/locale.svelte.ts");
