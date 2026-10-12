// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	LOGO_CANDIDATES,
	detectionPaths,
	hueFor,
	isRepoPath,
	mimeOf,
	monogram,
	resolveIcon,
	searchIcons,
	trayIconOf,
} from "./identity.ts";
import { hueOf } from "./space.ts";

const none = () => null;

test("monogram: two words → two letters, one word → one, separators and camelCase split", () => {
	expect(monogram("orbit-shop")).toBe("OS");
	expect(monogram("acme.dev")).toBe("AD");
	expect(monogram("atlas")).toBe("A");
	expect(monogram("codeAtlas")).toBe("CA");
	expect(monogram("my_cool project x")).toBe("MC");
	expect(monogram("ação")).toBe("A");
	expect(monogram("--")).toBe("?");
	expect(monogram("")).toBe("?");
});

test("resolution order: user choice, then detected logo, then monogram", () => {
	const detected = "blob:logo";
	expect(resolveIcon("shop", { icon: { kind: "emoji", value: "🚀" } }, detected)).toEqual({
		kind: "emoji",
		value: "🚀",
	});
	expect(resolveIcon("shop", { icon: { kind: "lucide", value: "rocket" } }, detected)).toEqual({
		kind: "lucide",
		value: "rocket",
	});
	expect(resolveIcon("shop", null, detected)).toEqual({ kind: "image", src: detected, auto: true });
	expect(resolveIcon("shop", undefined, null)).toEqual({ kind: "monogram", text: "S" });
	// a colour-only identity does not hide the detected logo
	expect(resolveIcon("shop", { hue: 10 }, detected).kind).toBe("image");
});

test("a chosen image wins when it loads and falls through when it does not", () => {
	const choice = { icon: { kind: "image", value: "public/logo.svg" } };
	const loaded = (p) => (p === "public/logo.svg" ? "blob:own" : null);
	expect(resolveIcon("shop", choice, "blob:auto", loaded)).toEqual({
		kind: "image",
		src: "blob:own",
		auto: false,
	});
	expect(resolveIcon("shop", choice, "blob:auto", none)).toEqual({
		kind: "image",
		src: "blob:auto",
		auto: true,
	});
	expect(resolveIcon("shop-web", choice, null, none)).toEqual({ kind: "monogram", text: "SW" });
});

test("a blank emoji is ignored", () => {
	expect(resolveIcon("shop", { icon: { kind: "emoji", value: "  " } }, null).kind).toBe("monogram");
});

test("detection checks package.json icon first, then the fixed list in order, without duplicates", () => {
	const paths = detectionPaths(JSON.stringify({ icon: "./brand/mark.png" }));
	expect(paths[0]).toBe("brand/mark.png");
	expect(paths.slice(1)).toEqual([...LOGO_CANDIDATES]);
	expect(new Set(paths).size).toBe(paths.length);
	// the icon already in the list is not repeated
	expect(detectionPaths(JSON.stringify({ icon: "public/favicon.svg" }))).toEqual([...LOGO_CANDIDATES]);
});

test("detection ignores unusable package.json icons", () => {
	for (const pj of [
		null,
		"not json",
		"{}",
		JSON.stringify({ icon: 3 }),
		JSON.stringify({ icon: "../outside.png" }),
		JSON.stringify({ icon: "/abs.png" }),
		JSON.stringify({ icon: "notes.txt" }),
	])
		expect(detectionPaths(pj)).toEqual([...LOGO_CANDIDATES]);
});

test("the candidate list covers the documented locations", () => {
	for (const p of [
		"public/favicon.svg",
		"public/favicon.ico",
		"public/favicon.png",
		"app/icon.png",
		"assets/logo.svg",
		".github/logo.png",
		"public/apple-touch-icon.png",
	])
		expect(LOGO_CANDIDATES).toContain(p);
	// vector before raster at the same place
	expect(LOGO_CANDIDATES.indexOf("public/favicon.svg")).toBeLessThan(
		LOGO_CANDIDATES.indexOf("public/favicon.png"),
	);
	for (const p of LOGO_CANDIDATES) {
		expect(isRepoPath(p)).toBe(true);
		expect(mimeOf(p)).not.toBeNull();
	}
});

test("repo paths never escape the root; mime by extension", () => {
	expect(isRepoPath("a/b.png")).toBe(true);
	expect(isRepoPath("../a.png")).toBe(false);
	expect(isRepoPath("a/../../b.png")).toBe(false);
	expect(isRepoPath("/etc/x.png")).toBe(false);
	expect(isRepoPath("")).toBe(false);
	expect(mimeOf("x/Logo.SVG")).toBe("image/svg+xml");
	expect(mimeOf("favicon.ico")).toBe("image/x-icon");
	expect(mimeOf("readme.md")).toBeNull();
});

test("hueFor: valid user hue wins, otherwise the spread hue, otherwise the derived one", () => {
	expect(hueFor("p", { hue: 200 }, 10)).toBe(200);
	expect(hueFor("p", { hue: 0 }, 10)).toBe(0);
	expect(hueFor("p", null, 10)).toBe(10);
	expect(hueFor("p", { icon: { kind: "emoji", value: "x" } }, 10)).toBe(10);
	expect(hueFor("p", undefined, undefined)).toBe(hueOf("p"));
	for (const bad of [360, -1, 12.5]) expect(hueFor("p", { hue: bad }, 10)).toBe(10);
});

test("icon search: prefix matches first, substring next, capped, empty query lists the start", () => {
	const names = ["git-branch", "rocket", "branch", "tree-pine", "brain", "bran-flakes"];
	expect(searchIcons(names, "bran")).toEqual(["branch", "bran-flakes", "git-branch"]);
	expect(searchIcons(names, "  Git Branch ")).toEqual(["git-branch"]);
	expect(searchIcons(names, "zzz")).toEqual([]);
	expect(searchIcons(names, "", 2)).toEqual(["git-branch", "rocket"]);
	expect(searchIcons(names, "r", 2)).toHaveLength(2);
});

test("tray icon: emoji and image pass through; lucide falls back to the monogram", () => {
	const data = "data:image/png;base64,AAAA";
	expect(trayIconOf({ kind: "emoji", value: "🚀" }, "shop", null)).toEqual({ kind: "emoji", text: "🚀" });
	expect(trayIconOf({ kind: "image", src: "blob:x", auto: true }, "shop", data)).toEqual({
		kind: "image",
		src: data,
	});
	expect(trayIconOf({ kind: "image", src: "blob:x", auto: true }, "shop-web", null)).toEqual({
		kind: "monogram",
		text: "SW",
	});
	expect(trayIconOf({ kind: "lucide", value: "rocket" }, "orbit-shop", null)).toEqual({
		kind: "monogram",
		text: "OS",
	});
	expect(trayIconOf({ kind: "monogram", text: "Z" }, "zed", null)).toEqual({ kind: "monogram", text: "Z" });
});
