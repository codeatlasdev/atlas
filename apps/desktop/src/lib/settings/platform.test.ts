// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Incident (Windows, 1.5.0-alpha.24 first-run pass): Settings listed Capture (Screen Recording access, "Open System
 * Settings", ⌘/⌥/⌃ keys) and the Terminal page listed the GPU terminal's own options, none of which exist off macOS;
 * opening Capture also raised "Screen capture is not available on this platform yet". What a platform shows is
 * decided in one place, `forPlatform`, which the sidebar, the search index and the deep links all read.
 */
import { describe, expect, test } from "bun:test";
import { REGISTRY, forPlatform, searchSettings, type CategoryDef } from "./index.ts";

const ids = (defs: readonly CategoryDef[]) =>
	defs.flatMap((c) => c.pages.flatMap((p) => p.groups.flatMap((g) => g.items.map((i) => i.id))));

/**
 * Words that only mean something on a Mac, in anything a Windows or Linux user can read. Key glyphs are not listed:
 * the registry spells keys through `kbd`, which `keys.test.ts` pins per platform (this suite runs as a Mac).
 */
const MAC_WORDS = /macOS|System Settings|Finder|Spotlight|Screen Recording|\bMac\b|Option key/;

describe("forPlatform", () => {
	test("macOS sees the whole registry", () => {
		expect(forPlatform(REGISTRY, true)).toBe(REGISTRY);
	});

	test("off macOS the Capture category is gone, with every one of its rows", () => {
		const other = forPlatform(REGISTRY, false);
		expect(other.map((c) => c.id)).not.toContain("capture");
		expect(ids(other).filter((id) => id.startsWith("capture-"))).toEqual([]);
		expect(ids(REGISTRY).filter((id) => id.startsWith("capture-")).length).toBeGreaterThan(10);
	});

	test("off macOS the fast-terminal rows are gone and the rest of the Terminal page stays", () => {
		const terminal = forPlatform(REGISTRY, false).find((c) => c.id === "terminal");
		const rows = ids([terminal]);
		for (const id of ["gpu", "ligatures", "contrast", "scrollback-memory", "click-to-move", "option-as-alt"])
			expect(rows, id).not.toContain(id);
		for (const id of ["host-restart", "term-font", "scrollback", "copy-on-select", "clipboard"])
			expect(rows, id).toContain(id);
	});

	test("a group or page left with no rows drops out instead of rendering an empty card", () => {
		const defs: CategoryDef[] = [
			{
				id: "a",
				label: "A",
				description: "A.",
				cluster: "app",
				pages: [
					{
						files: ["x"],
						groups: [
							{ title: "Only mac", items: [{ id: "m", label: "M", description: "M.", macOnly: true }] },
							{ title: "Both", items: [{ id: "b", label: "B", description: "B." }] },
						],
					},
					{
						id: "t",
						files: ["y"],
						groups: [
							{ title: "Only mac", items: [{ id: "n", label: "N", description: "N.", macOnly: true }] },
						],
					},
				],
			},
		];
		const [a] = forPlatform(defs, false);
		expect(a.pages).toHaveLength(1);
		expect(a.pages[0].groups.map((g) => g.title)).toEqual(["Both"]);
	});

	test("no label, description or keyword a non-Mac user can find names a Mac key or a macOS window", () => {
		for (const c of forPlatform(REGISTRY, false)) {
			expect(c.label + c.description, c.id).not.toMatch(MAC_WORDS);
			for (const p of c.pages)
				for (const g of p.groups)
					for (const i of g.items)
						expect(`${i.label} ${i.description}`, i.id).not.toMatch(
							// "Option key as Alt" is hidden off macOS (macOnly); anything else naming a Mac key is a leak.
							MAC_WORDS,
						);
		}
	});

	// The search index is built from `forPlatform`, so a hidden page cannot be found by any word it contains.
	test("a search over the non-Mac registry never finds capture or fast-terminal settings", () => {
		const index = forPlatform(REGISTRY, false).flatMap((c) =>
			c.pages.flatMap((p) =>
				p.groups.flatMap((g) =>
					g.items.map((i) => ({
						section: c.id,
						group: g.title,
						id: i.id,
						label: i.label,
						description: i.description,
						keywords: i.keywords,
					})),
				),
			),
		);
		for (const q of ["screenshot", "screen recording", "capture", "fast terminal", "graphics chip"])
			expect(
				searchSettings(q, index).map((e) => e.id),
				q,
			).toEqual([]);
		expect(searchSettings("scrollback", index).map((e) => e.id)).toContain("scrollback");
	});
});
