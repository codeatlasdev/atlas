import { defineCommand } from "citty";
import { loadConfig } from "../lib/config";

export default defineCommand({
	meta: {
		name: "logs",
		description: "Stream application logs",
	},
	args: {
		service: { type: "positional", description: "Service name", default: "server" },
		follow: { type: "boolean", alias: "f", description: "Follow log output", default: false },
		tail: { type: "string", alias: "n", description: "Number of lines", default: "100" },
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
		const service = args.service || "server";
		const follow = args.follow ? "-f" : "";
		const tail = args.tail || "100";
		const ns = config.domain ? config.domain.split(".")[0] : "app";

		let cmd: string;
		if (runtime === "swarm") {
			cmd = `docker service logs ${ns}_${service} --tail ${tail} ${follow} 2>&1`;
		} else {
			const targets: Record<string, string> = {
				server: "deploy/server",
				workers: "deploy/workers",
				postgres: "statefulset/postgres",
				redis: "deploy/redis",
			};
			const target = targets[service] || `deploy/${service}`;
			cmd = `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} logs ${target} --tail=${tail} ${follow} --all-containers 2>&1`;
		}

		const proc = Bun.spawn(
			["ssh", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=10", host, cmd],
			{ stdout: "inherit", stderr: "inherit" },
		);
		await proc.exited;
	},
});
