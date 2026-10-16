// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident #29: the Source control row for `.logs/` read `/logs.`. A path is clipped at its START (so the file name
// stays visible) with `direction: rtl` on the clipping box; in an RTL paragraph the Unicode bidi algorithm moves the
// weak/neutral punctuation at the string's edges (leading `.`, trailing `/`) to the other side. The text itself
// must therefore sit in an LTR isolate inside the RTL box: the DOM order of the characters is what the user reads.
const { flushSync, mount, unmount } = await import("svelte");
const { default: PathText } = await import("./PathText.svelte");

function render(path: string) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(PathText, { target, props: { path } }));
	return { target, done: () => (unmount(app), target.remove()) };
}

// The paths that garble: edge punctuation, a trailing slash, a lone dotfile, an absolute path, `~`, and Hebrew/Arabic
// names (an isolate must keep them whole instead of re-ordering the surrounding path).
const PATHS = [
	".logs/",
	".logs/api.log",
	".env",
	"/usr/local/bin/",
	"~/Projects/atlas",
	"src/(group)/[id]/+page.svelte",
	"docs/שלום.md",
];

test.each(PATHS)("%p is shown verbatim inside an LTR isolate, so punctuation cannot be reordered", (p) => {
	const { target, done } = render(p);
	const text = target.querySelector("bdi");
	expect(text).not.toBeNull();
	expect(text.textContent).toBe(p);
	expect(text.getAttribute("dir")).toBe("ltr");
	expect(target.textContent).toBe(p);
	done();
});

test("the clipping box keeps start-side ellipsis (rtl) while only the inner text is forced LTR", () => {
	const { target, done } = render("a/b/c.ts");
	const box = target.querySelector(".path");
	expect(box.getAttribute("dir")).toBe("rtl");
	expect(box.querySelector("bdi").getAttribute("dir")).toBe("ltr");
	done();
});
