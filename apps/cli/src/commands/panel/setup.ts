import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadConfig, saveConfig } from "../../lib/config"

export default defineCommand({
	meta: { name: "setup", description: "Connect CLI to Atlas Control Panel" },
	args: {
		url: { type: "string", description: "Control Panel URL" },
		token: { type: "string", description: "Auth token" },
	},
	async run({ args }) {
		p.intro(pc.bgMagenta(pc.black(" atlas panel setup ")))

		const config = await loadConfig()

		const url = args.url || await p.text({
			message: "Control Panel URL",
			placeholder: "https://atlas.codeatlas.dev",
			initialValue: config.panelUrl || "",
			validate: (v) => {
				if (!v) return "Required"
				if (!v.startsWith("http")) return "Must start with http:// or https://"
			},
		})
		if (p.isCancel(url)) return

		// Verify connection
		const spinner = p.spinner()
		spinner.start("Verifying connection...")

		try {
			const res = await fetch(`${url}/health`)
			if (!res.ok) throw new Error(`HTTP ${res.status}`)
			const data = await res.json() as { status: string; version: string }
			spinner.stop(`Connected to Atlas Panel v${data.version}`)
		} catch (e) {
			spinner.stop(`Cannot reach ${url}`)
			console.error(e instanceof Error ? e.message : e)
			return
		}

		const token = args.token || await p.text({
			message: "Auth token (from atlas panel login or seed)",
			initialValue: config.panelToken || "",
			validate: (v) => { if (!v) return "Required" },
		})
		if (p.isCancel(token)) return

		// Verify token
		spinner.start("Verifying token...")
		try {
			const res = await fetch(`${url}/servers`, {
				headers: { Authorization: `Bearer ${token}` },
			})
			if (!res.ok) throw new Error(`HTTP ${res.status}`)
			spinner.stop("Token valid ✓")
		} catch {
			spinner.stop("Invalid token")
			return
		}

		await saveConfig({ panelUrl: url as string, panelToken: token as string })
		p.outro(pc.green("Control Panel configured! Deploy will now use the API."))
	},
})
