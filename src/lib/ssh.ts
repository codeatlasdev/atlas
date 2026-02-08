import { $ } from "bun"
import { mkdir } from "node:fs/promises"
import { join } from "node:path"

interface SSHResult {
	ok: boolean
	stdout: string
	stderr: string
	exitCode: number
}

const CONTROL_DIR = join(process.env.HOME || "/tmp", ".atlas", "ssh")

// SSH with ControlMaster — reuses connections, eliminates repeated handshakes
export async function ssh(host: string, command: string): Promise<SSHResult> {
	await mkdir(CONTROL_DIR, { recursive: true })
	const controlPath = join(CONTROL_DIR, `%r@%h:%p`)

	try {
		const result =
			await $`ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 -o ControlMaster=auto -o ControlPath=${controlPath} -o ControlPersist=300 ${host} ${`bash -s <<'ATLAS_EOF'\n${command}\nATLAS_EOF`}`.quiet()
		return {
			ok: result.exitCode === 0,
			stdout: result.stdout.toString(),
			stderr: result.stderr.toString(),
			exitCode: result.exitCode,
		}
	} catch (e: unknown) {
		const err = e as { stdout?: Buffer; stderr?: Buffer; exitCode?: number }
		return {
			ok: false,
			stdout: err.stdout?.toString() ?? "",
			stderr: err.stderr?.toString() ?? String(e),
			exitCode: err.exitCode ?? 1,
		}
	}
}

// Interactive SSH (for exec) — needs TTY
export async function sshInteractive(host: string, command: string): Promise<number> {
	await mkdir(CONTROL_DIR, { recursive: true })
	const controlPath = join(CONTROL_DIR, `%r@%h:%p`)

	const proc = Bun.spawn(
		["ssh", "-t",
			"-o", "StrictHostKeyChecking=accept-new",
			"-o", `ControlMaster=auto`,
			"-o", `ControlPath=${controlPath}`,
			"-o", "ControlPersist=300",
			host, command],
		{ stdin: "inherit", stdout: "inherit", stderr: "inherit" },
	)
	return proc.exited
}
