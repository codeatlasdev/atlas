import { defineCommand } from "citty"
import pc from "picocolors"
import { PanelClient } from "../../lib/panel"

export default defineCommand({
	meta: { name: "list", description: "List all servers" },
	async run() {
		const panel = await PanelClient.create()
		if (!panel) { console.error("Not connected. Run: atlas panel setup"); return }

		const servers = await panel.listServers()

		if (servers.length === 0) {
			console.log("No servers. Add one: atlas panel server add")
			return
		}

		for (const s of servers) {
			const icon = s.status === "online" ? pc.green("●") : s.status === "provisioning" ? pc.yellow("◌") : pc.red("●")
			console.log(`${icon} ${pc.bold(s.name)} (${s.host}) — ${s.status}`)
		}
	},
})
