// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Every agent Atlas can launch shows its official logo in the New-tab menu and on its tab. The agent ids come
 * from the Rust registry (`crates/atlas-desktop/src/agent/registry.rs`), read from the real source, so adding an
 * agent there without a logo here turns this red instead of silently falling back to the generic robot.
 */
import { expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { LOGO_FILES, agentLogo } from "./agentLogos.ts";

const registry = readFileSync(
	join(import.meta.dir, "../../../../crates/atlas-desktop/src/agent/registry.rs"),
	"utf8",
);
const ids = [...registry.matchAll(/^\s*id:\s*"([^"]+)",/gm)].map((m) => m[1]);
const dir = join(import.meta.dir, "agent-logos");

test("the registry parse found the agents (guards the regex, not the logos)", () => {
	expect(ids).toEqual(expect.arrayContaining(["omp", "claude", "codex"]));
});

test("every agent id in the Rust registry maps to a logo", () => {
	const missing = ids.filter((id) => !agentLogo(id));
	expect(missing).toEqual([]);
});

test("an unknown agent has no logo, so callers fall back to the generic icon", () => {
	expect(agentLogo("not-an-agent")).toBeNull();
	expect(agentLogo(undefined)).toBeNull();
	expect(agentLogo("")).toBeNull();
});

test("the monochrome Codex blossom ships a light and a dark variant; colored marks need only one", () => {
	expect(agentLogo("codex")?.dark).toBeTruthy();
	expect(agentLogo("codex")?.dark).not.toBe(agentLogo("codex")?.light);
	expect(agentLogo("claude")?.dark).toBeUndefined();
	expect(agentLogo("omp")?.dark).toBeUndefined();
});

test("the rendered markup is byte-identical to the documented asset files", () => {
	for (const [name, svg] of Object.entries(LOGO_FILES)) {
		expect(svg).toBe(readFileSync(join(dir, name), "utf8").trim());
	}
});

test("every asset file is rendered and listed with source + license in the README", () => {
	const files = readdirSync(dir).filter((f) => f.endsWith(".svg"));
	expect(files.sort()).toEqual(Object.keys(LOGO_FILES).sort());
	const readme = readFileSync(join(dir, "README.md"), "utf8");
	for (const f of files) expect(readme).toContain(f);
});

test("marks are never recolored with currentColor (brand rules forbid alteration)", () => {
	for (const svg of Object.values(LOGO_FILES)) expect(svg).not.toContain("currentColor");
});

test("the logo is decorative: inline SVG carries no script or external reference", () => {
	for (const svg of Object.values(LOGO_FILES)) {
		expect(svg).not.toMatch(/<script|href=|xlink:|<image/i);
	}
});
