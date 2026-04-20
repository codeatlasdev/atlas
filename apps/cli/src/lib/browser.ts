import { $ } from "bun";

/** Open a URL in the user's default browser. Handles macOS, Linux, and WSL. */
export async function openBrowser(url: string): Promise<boolean> {
	const commands = getOpenCommands();
	for (const cmd of commands) {
		try {
			const proc = Bun.spawn([cmd, url], { stdout: "ignore", stderr: "ignore" });
			const code = await proc.exited;
			if (code === 0) return true;
		} catch {}
	}
	return false;
}

function getOpenCommands(): string[] {
	if (process.platform === "darwin") return ["open"];
	if (isWSL()) return ["wslview", "cmd.exe /c start"];
	return ["xdg-open"];
}

function isWSL(): boolean {
	try {
		const proc = Bun.spawnSync(["cat", "/proc/version"], { stdout: "pipe" });
		const version = new TextDecoder().decode(proc.stdout).toLowerCase();
		return version.includes("microsoft") || version.includes("wsl");
	} catch {
		return false;
	}
}
