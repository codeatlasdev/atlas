import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { PanelClient } from "../../lib/panel"

export default defineCommand({
	meta: { name: "add", description: "Add and provision a new server" },
	args: {
		name: { type: "string", description: "Server name" },
		host: { type: "string", description: "SSH host (e.g., root@1.2.3.4)" },
		domain: { type: "string", description: "Base domain for this server" },
		"no-provision": { type: "boolean", description: "Skip provisioning", default: false },
	},
	async run({ args }) {
		const panel = await PanelClient.create()
		if (!panel) { console.error("Not connected. Run: atlas panel setup"); return }

		p.intro(pc.bgMagenta(pc.black(" atlas panel server add ")))

		const name = args.name || await p.text({ message: "Server name", placeholder: "production" }) as string
		if (p.isCancel(name)) return

		const host = args.host || await p.text({ message: "SSH host", placeholder: "root@1.2.3.4" }) as string
		if (p.isCancel(host)) return

		const domain = args.domain || await p.text({ message: "Base domain", placeholder: "myapp.com" }) as string
		if (p.isCancel(domain)) return

		const provision = !args["no-provision"]

		const spinner = p.spinner()
		spinner.start(provision ? "Adding server and starting provisioning..." : "Adding server...")

		try {
			const server = await panel.req<{ id: number; name: string; status: string }>("/servers", {
				method: "POST",
				body: JSON.stringify({ name, host, domain, provision }),
			})

			if (provision) {
				spinner.stop(`Server ${server.name} added (id: ${server.id}) — provisioning in background`)
				p.note("Provisioning takes 5-10 minutes.\nCheck status: atlas panel status", "Provisioning started")
			} else {
				spinner.stop(`Server ${server.name} added (id: ${server.id})`)
			}
		} catch (e) {
			spinner.stop(`Failed: ${e instanceof Error ? e.message : e}`)
		}

		p.outro(pc.green("Done"))
	},
})
