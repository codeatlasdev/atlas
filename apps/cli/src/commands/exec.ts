import { defineCommand } from "citty";
import { loadConfig } from "../lib/config";

export default defineCommand({
	meta: {
		name: "exec",
		description: "Execute a command in a running container",
	},
	args: {
		service: { type: "positional", description: "Service name", default: "server" },
		command: { type: "string", alias: "c", description: "Command to run", default: "sh" },
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig();
		const host = args.host || config.host;
		if (!host) {
			console.error("No host configured. Run: atlas infra setup");
			return;
		}

		const runtime = config.runtime || "k3s";
		const ns = config.domain ? config.domain.split(".")[0] : "app";
		const service = args.service || "server";
		const command = args.command || "sh";

		let cmd: string;
		if (runtime === "swarm") {
			cmd = `docker exec -it $(docker ps -q -f "name=${ns}_${service}" | head -1) ${command}`;
		} else {
			const targets: Record<string, string> = {
				server: "deploy/server",
				workers: "deploy/workers",
				postgres: "statefulset/postgres",
				redis: "deploy/redis",
			};
			const target = targets[service] || `deploy/${service}`;
			cmd = `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} exec -it ${target} -- ${command}`;
		}

		const proc = Bun.spawn(["ssh", "-t", "-o", "StrictHostKeyChecking=accept-new", host, cmd], {
			stdout: "inherit",
			stderr: "inherit",
			stdin: "inherit",
		});
		process.exit(await proc.exited);
	},
});
