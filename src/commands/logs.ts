import { defineCommand } from "citty"
import { loadConfig } from "../lib/config"

export default defineCommand({
	meta: {
		name: "logs",
		description: "Stream application logs",
	},
	args: {
		service: {
			type: "positional",
			description: "Service name: server, workers, postgres, redis",
			default: "server",
		},
		follow: {
			type: "boolean",
			alias: "f",
			description: "Follow log output",
			default: false,
		},
		tail: {
			type: "string",
			alias: "n",
			description: "Number of lines to show",
			default: "100",
		},
		host: {
			type: "string",
			description: "SSH host (uses saved config if omitted)",
		},
	},
	async run({ args }) {
		const config = await loadConfig()
		const host = args.host || config.host
		if (!host) {
			console.error("No host configured. Run: atlas infra setup")
			return
		}

		const service = args.service || "server"
		const follow = args.follow ? "-f" : ""
		const tail = args.tail || "100"

		// Map service names to K8s resources
		const targets: Record<string, string> = {
			server: "deploy/server",
			workers: "deploy/workers",
			postgres: "statefulset/postgres",
			redis: "deploy/redis",
		}

		const target = targets[service] || `deploy/${service}`

		const ns = config.domain ? config.domain.split(".")[0] : "app"

		// Stream via SSH — spawn directly so output flows to terminal
		const cmd = `ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 ${host} "export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} logs ${target} --tail=${tail} ${follow} --all-containers 2>&1"`

		const proc = Bun.spawn(["bash", "-c", cmd], {
			stdout: "inherit",
			stderr: "inherit",
		})

		await proc.exited
	},
})
