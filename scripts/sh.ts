// sh.ts — runs a repo shell script with the right bash on every OS: `bun scripts/sh.ts scripts/x.sh [args…]`.
//
// Windows: Bun's script runner cannot execute a `.sh` by path, and the `bash` first on PATH is often WSL's
// (`WindowsApps\bash.exe`), which runs the script inside Linux where the Windows Rust and Bun do not exist.
// So on Windows this always uses Git for Windows' bash, found next to `git`; `ATLAS_BASH` overrides it.
// Elsewhere it is plain `bash`.
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";

/** The bash executable to run repo scripts with. */
export function bash(): string {
	if (process.env.ATLAS_BASH) return process.env.ATLAS_BASH;
	if (process.platform !== "win32") return "bash";
	// `git --exec-path` is `<git>/<mingw64|ucrt64|…>/libexec/git-core`; bash is `<git>/bin/bash.exe`.
	const exec = Bun.spawnSync(["git", "--exec-path"]).stdout.toString().trim();
	const candidates = [
		exec && join(dirname(dirname(dirname(exec))), "bin", "bash.exe"),
		join(process.env.ProgramFiles ?? "C:\\Program Files", "Git", "bin", "bash.exe"),
	].filter(Boolean) as string[];
	const found = candidates.find((p) => existsSync(p));
	if (!found) {
		console.error("✗ Git Bash not found (install Git for Windows: https://git-scm.com/download/win, or set ATLAS_BASH)");
		process.exit(1);
	}
	return found;
}

/** argv that runs `script` with `args` through `bash()`. */
export function shArgv(script: string, ...args: string[]): string[] {
	return [bash(), script, ...args];
}

if (import.meta.main) {
	const [script, ...args] = process.argv.slice(2);
	if (!script) {
		console.error("usage: bun scripts/sh.ts <script.sh> [args…]");
		process.exit(2);
	}
	const p = Bun.spawnSync(shArgv(script, ...args), { stdio: ["inherit", "inherit", "inherit"] });
	process.exit(p.exitCode ?? 1);
}
