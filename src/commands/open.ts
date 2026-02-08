import { defineCommand } from "citty"
import { loadConfig } from "../lib/config"

const targets: Record<string, (domain: string) => string> = {
	app: (d) => `https://backoffice.${d}`,
	api: (d) => `https://api.${d}`,
	grafana: (d) => `https://grafana.${d}`,
	argocd: (d) => `https://argocd.${d}`,
	backoffice: (d) => `https://backoffice.${d}`,
	finances: (d) => `https://finances.${d}`,
	bi: (d) => `https://bi.${d}`,
}

export default defineCommand({
	meta: { name: "open", description: "Open a service URL in the browser" },
	args: {
		target: {
			type: "positional",
			description: "What to open: app, api, grafana, argocd, backoffice, finances, bi",
			default: "app",
		},
	},
	async run({ args }) {
		const config = await loadConfig()
		const domain = config.domain
		if (!domain) {
			console.error("No domain configured. Run: atlas infra setup")
			return
		}
		const target = (args.target as string) || "app"

		const urlFn = targets[target]
		if (!urlFn) {
			console.error(`Unknown target: ${target}. Options: ${Object.keys(targets).join(", ")}`)
			return
		}

		const url = urlFn(domain)
		console.log(`→ Opening ${url}`)

		const cmd = process.platform === "darwin" ? "open" : "xdg-open"
		Bun.spawn([cmd, url], { stdout: "ignore", stderr: "ignore" })
	},
})
