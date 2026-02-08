import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadConfig } from "../../lib/config"
import { PanelClient } from "../../lib/panel"

export default defineCommand({
	meta: { name: "status", description: "Show Control Panel connection status" },
	async run() {
		const config = await loadConfig()

		if (!config.panelUrl || !config.panelToken) {
			console.log(pc.yellow("Not connected to Control Panel"))
			console.log("Run: atlas panel setup")
			return
		}

		console.log(`${pc.bold("Panel:")} ${config.panelUrl}`)

		try {
			const panel = new PanelClient(config.panelUrl, config.panelToken)
			const servers = await panel.listServers()
			const projects = await panel.listProjects()

			console.log(`${pc.bold("Servers:")} ${servers.length}`)
			for (const s of servers) {
				const icon = s.status === "online" ? pc.green("●") : pc.yellow("●")
				console.log(`  ${icon} ${s.name} (${s.host}) — ${s.status}`)
			}

			console.log(`${pc.bold("Projects:")} ${projects.length}`)
			for (const proj of projects) {
				console.log(`  ${pc.cyan(proj.name)} ${proj.domain ? `→ ${proj.domain}` : ""}`)
			}
		} catch (e) {
			console.error(pc.red("Failed to connect:"), e instanceof Error ? e.message : e)
		}
	},
})
