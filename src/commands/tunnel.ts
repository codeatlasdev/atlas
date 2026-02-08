import { defineCommand } from "citty"
import { $ } from "bun"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadConfig, ATLAS_DIR } from "../lib/config"
import { loadProject } from "../lib/project"
import { join } from "node:path"
import { mkdir } from "node:fs/promises"

const BIN_DIR = join(ATLAS_DIR, "bin")

async function ensureCloudflared(): Promise<string> {
	const bin = join(BIN_DIR, "cloudflared")
	const file = Bun.file(bin)
	if (await file.exists()) return bin

	const platform = process.platform === "darwin" ? "darwin" : "linux"
	const arch = process.arch === "arm64" ? "arm64" : "amd64"
	const url = `https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-${platform}-${arch}`

	await mkdir(BIN_DIR, { recursive: true })
	const res = await fetch(url, { redirect: "follow" })
	if (!res.ok) throw new Error(`Failed to download cloudflared: ${res.status}`)
	const bytes = await res.arrayBuffer()
	await Bun.write(bin, bytes)
	await $`chmod +x ${bin}`.quiet()
	return bin
}

function parseQuickTunnelURL(output: string): string | null {
	const match = output.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/)
	return match?.[0] || null
}

export default defineCommand({
	meta: { name: "tunnel", description: "Expose local services via Cloudflare Tunnel" },
	args: {
		service: { type: "string", alias: "s", description: "Service name from atlas.yaml" },
		port: { type: "string", alias: "p", description: "Local port (overrides atlas.yaml)" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes
		const project = await loadProject()

		// Determine port
		let port = args.port ? Number.parseInt(args.port) : null
		let serviceName = args.service || null

		if (!port && project) {
			// Pick service: explicit, or first api service, or first with port
			const entries = Object.entries(project.services)
			let entry = serviceName ? entries.find(([n]) => n === serviceName) : null
			if (!entry) entry = entries.find(([, s]) => s.type === "api" && s.port)
			if (!entry) entry = entries.find(([, s]) => s.port)
			if (entry) {
				serviceName = entry[0]
				port = entry[1].port!
			}
		}

		if (!port) {
			if (auto) { console.error("No port. Use --port or add port to atlas.yaml"); return }
			const input = await p.text({
				message: "Local port to expose",
				placeholder: "3001",
				validate: (v) => (Number.isNaN(Number(v)) ? "Must be a number" : undefined),
			})
			if (p.isCancel(input)) return
			port = Number(input)
		}

		if (!auto) p.intro(pc.bgCyan(pc.black(" atlas tunnel ")))

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner()

		log.start("Checking cloudflared...")
		let bin: string
		try {
			bin = await ensureCloudflared()
			log.stop("cloudflared ready")
		} catch (e) {
			log.stop(`Failed to get cloudflared: ${e}`)
			return
		}

		const label = serviceName ? `${serviceName} (localhost:${port})` : `localhost:${port}`
		console.log(`\n  ${pc.dim("Exposing")} ${pc.bold(label)} ${pc.dim("via Quick Tunnel...")}\n`)

		// Run cloudflared — it prints the URL to stderr
		const proc = Bun.spawn([bin, "tunnel", "--url", `http://localhost:${port}`], {
			stdout: "pipe",
			stderr: "pipe",
		})

		// Read stderr to find the URL
		const reader = proc.stderr.getReader()
		const decoder = new TextDecoder()
		let url: string | null = null
		let buffer = ""

		const urlTimeout = setTimeout(() => {
			if (!url) console.log(pc.dim("  Waiting for tunnel connection..."))
		}, 3000)

		try {
			while (true) {
				const { done, value } = await reader.read()
				if (done) break
				buffer += decoder.decode(value, { stream: true })

				if (!url) {
					url = parseQuickTunnelURL(buffer)
					if (url) {
						clearTimeout(urlTimeout)
						console.log(`  ${pc.green("●")} ${pc.bold(url)}\n`)
						console.log(pc.dim("  Press Ctrl+C to stop\n"))
					}
				}
			}
		} catch {
			// Process killed (Ctrl+C)
		}

		clearTimeout(urlTimeout)

		// Cleanup
		proc.kill()
		await proc.exited

		console.log(`\n${pc.dim("Tunnel closed")}`)
	},
})
