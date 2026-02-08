import { defineCommand } from "citty"
import pc from "picocolors"
import { ssh } from "../lib/ssh"
import { loadConfig } from "../lib/config"

export default defineCommand({
	meta: { name: "scale", description: "Scale a service" },
	args: {
		service: {
			type: "positional",
			description: "Service name: server, workers",
			required: true,
		},
		replicas: {
			type: "string",
			alias: "r",
			description: "Number of replicas",
			required: true,
		},
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const service = args.service as string
		const replicas = args.replicas as string

		console.log(`→ Scaling ${service} to ${replicas} replicas...`)
		const result = await ssh(host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} scale deploy/${service} --replicas=${replicas} 2>&1
kubectl -n ${ns} rollout status deploy/${service} --timeout=60s 2>&1`)

		if (!result.ok) {
			console.error(result.stderr || result.stdout)
			return
		}
		console.log(`${pc.green("✓")} ${service} scaled to ${replicas} replicas`)
	},
})
