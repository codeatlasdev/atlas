import { defineCommand } from "citty"
import { loadConfig } from "../lib/config"

export default defineCommand({
	meta: {
		name: "exec",
		description: "Execute a command in a running container",
	},
	args: {
		service: {
			type: "positional",
			description: "Service name: server, workers, postgres, redis",
			default: "server",
		},
		command: {
			type: "string",
			alias: "c",
			description: "Command to run (default: sh)",
			default: "sh",
		},
		host: {
			type: "string",
			description: "SSH host (uses saved config if omitted)",
		},
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) {
			console.error("No host configured. Run: atlas infra setup")
			return
		}

		const service = args.service || "server"
		const command = args.command || "sh"

		const targets: Record<string, string> = {
			server: "deploy/server",
			workers: "deploy/workers",
			postgres: "statefulset/postgres",
			redis: "deploy/redis",
		}

		const target = targets[service] || `deploy/${service}`

		const proc = Bun.spawn(
			["ssh", "-t", "-o", "StrictHostKeyChecking=accept-new", host,
				`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} exec -it ${target} -- ${command}`],
			{ stdout: "inherit", stderr: "inherit", stdin: "inherit" },
		)

		process.exit(await proc.exited)
	},
})
