// Throwaway repos for the flicker sweep (`scripts/flicker-sweep.ts`): the shape of studio's orbit-shop demo (a stack with
// a quiet database, a web and api, a service that crash-loops, a one-shot migration, an action) in git repos under /tmp,
// plus two small neighbours so there are spaces to switch between, and an uncommitted change so Changes has a diff.
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const ATLAS = join(import.meta.dir, "../target/debug/atlas");

const BASE = "/private/tmp/flicker-sweep-fixtures";

const git = (cwd: string, ...a: string[]) =>
	execFileSync("git", ["-c", "user.name=Sweep", "-c", "user.email=sweep@atlas.invalid", "-c", "commit.gpgsign=false", ...a], { cwd, stdio: "ignore" });

/** A service that logs forever, so the Panel cards and Logs keep updating while a surface is recorded. */
const ticker = (what: string, port: number) =>
	`  ${what}:\n    command: "i=0; while true; do i=$((i+1)); echo '${what} tick '$i; sleep 0.5; done"\n    port: ${port}\n    ready_log: tick\n`;

function seed(dir: string, yaml: string, files: Record<string, string>) {
	mkdirSync(dir, { recursive: true });
	writeFileSync(join(dir, "atlas.yaml"), yaml);
	writeFileSync(join(dir, ".gitignore"), ".logs/\n");
	for (const [f, body] of Object.entries(files)) {
		mkdirSync(join(dir, f, ".."), { recursive: true });
		writeFileSync(join(dir, f), body);
	}
	git(dir, "init", "-q", "-b", "main");
	git(dir, "add", "-A");
	git(dir, "commit", "-q", "-m", "initial commit");
}

export interface Fixtures {
	/** project roots, the first is the one the sweep works in */
	roots: string[];
	/** name of the project every Panel/Logs/Changes surface uses */
	main: string;
	/** a service of `main` that exits non-zero over and over (the failed card) */
	failing: string;
	cleanup(): void;
}

export function fixtures(): Fixtures {
	rmSync(BASE, { recursive: true, force: true });
	const main = join(BASE, "sweep-shop");
	seed(
		main,
		`name: sweep-shop
services:
${ticker("database", 24100)}${ticker("web", 24101)}${ticker("api", 24102)}  payments:
    command: "echo 'payments: connection refused' >&2; sleep 1; exit 3"
actions:
  - key: x
    label: Migrate
    command: "echo migrating; sleep 1; echo done"
`,
		{
			"src/cart.ts": "export const total = (xs: number[]) => xs.reduce((a, b) => a + b, 0);\n",
			"src/checkout.ts": "export const ship = (n: number) => n * 2;\n",
			"README.md": "# sweep-shop\n",
		},
	);
	// an uncommitted change in two files: Changes has something to show (and to scroll)
	writeFileSync(join(main, "src/cart.ts"), `${"// line\n".repeat(60)}export const total = (xs: number[]) => xs.reduce((a, b) => a + b, 0) + 1;\n`);
	writeFileSync(join(main, "src/checkout.ts"), "export const ship = (n: number) => n * 3;\nexport const tax = (n: number) => n * 0.2;\n");
	const others = ["sweep-docs", "sweep-api"].map((n, i) => {
		const d = join(BASE, n);
		seed(d, `name: ${n}\nservices:\n${ticker("app", 24200 + i)}`, { "README.md": `# ${n}\n` });
		return d;
	});
	const roots = [main, ...others];
	return {
		roots,
		main: "sweep-shop",
		failing: "payments",
		// The app starts each stack as a detached `atlas dev --runner` (ppid 1) that outlives the agent instance: stop them with
		// the product's own `atlas down` (it reads the runner's pid from the project's .logs) before the directory goes.
		cleanup() {
			for (const root of roots) spawnSync(ATLAS, ["down", "--dir", root], { stdio: "ignore", timeout: 30_000 });
			rmSync(BASE, { recursive: true, force: true });
		},
	};
}
