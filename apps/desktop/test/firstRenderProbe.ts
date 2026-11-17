// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Run by `locale.test.ts` in a FRESH process (`bun --preload test/setup.ts`), because the trap it checks happens once
 * per process: Paraglide calls the strategy's `setLocale` the first time `getLocale()` runs, and that first run must be
 * inside a template that renders a message. Prints one JSON line.
 */
const { flushSync, mount } = await import("svelte");
const { setAppLocale } = await import("#lib/i18n/locale.svelte.ts");
const { default: Harness } = await import("./MessageHarness.svelte");

const target = document.createElement("div");
document.body.append(target);
const out = { first: "", switched: "", sameNode: false, error: null };
try {
	// No `m.*()` has run in this process: the very first render is the first `getLocale()`.
	mount(Harness, { target, props: { count: 2, at: 0, now: 5 * 60_000 } });
	flushSync();
	const label = target.querySelector("#label");
	out.first = label.textContent;
	setAppLocale("pt-BR");
	flushSync();
	out.switched = `${target.querySelector("#label").textContent} | ${target.querySelector("#count").textContent} | ${target.querySelector("#ago").textContent}`;
	out.sameNode = label === target.querySelector("#label");
} catch (e) {
	out.error = String(e?.message ?? e);
}
console.log(JSON.stringify(out));
