import { defineCommand } from "citty"
import pc from "picocolors"
import { ssh } from "../lib/ssh"
import { loadConfig } from "../lib/config"

export default defineCommand({
	meta: {
		name: "restart",
		description: "Restart a service (rolling restart)",
	},
	args: {
		service: {
			type: "positional",
			description: "Service: server, workers, or 'all'",
			default: "server",
		},
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const service = (args.service as string) || "server"
		const services = service === "all" ? ["server", "workers"] : [service]

		for (const svc of services) {
			console.log(`→ Restarting ${svc}...`)
			const result = await ssh(host,
				`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} rollout restart deploy/${svc} 2>&1 && \
kubectl -n ${ns} rollout status deploy/${svc} --timeout=60s 2>&1`)

			if (!result.ok) {
				console.error(`  ${pc.red("✗")} ${svc}: ${result.stderr || result.stdout}`)
			} else {
				console.log(`  ${pc.green("✓")} ${svc} restarted`)
			}
		}
	},
})
