import { ssh } from "@atlas/ssh";
import { defineCommand } from "citty";
import pc from "picocolors";
import { loadConfig } from "../lib/config";
import { resolveNamespace } from "../lib/project";

export default defineCommand({
	meta: { name: "restart", description: "Restart a service (rolling restart)" },
	args: {
		service: { type: "positional", description: "Service or 'all'", default: "server" },
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig();
		const host = args.host || config.host;
		if (!host) {
			console.error("No host. Run: atlas infra setup");
			return;
		}

		const runtime = config.runtime || "k3s";
		const ns = await resolveNamespace(config.domain);
		const service = (args.service as string) || "server";
		const services = service === "all" ? ["server", "workers"] : [service];

		for (const svc of services) {
			console.log(`→ Restarting ${svc}...`);

			let cmd: string;
			if (runtime === "swarm") {
				cmd = `docker service update --force ${ns}_${svc} 2>&1`;
			} else {
				cmd = `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} rollout restart deploy/${svc} 2>&1 && \
kubectl -n ${ns} rollout status deploy/${svc} --timeout=60s 2>&1`;
			}

			const result = await ssh(host, cmd);
			if (!result.ok) {
				console.error(`  ${pc.red("✗")} ${svc}: ${result.stderr || result.stdout}`);
			} else {
				console.log(`  ${pc.green("✓")} ${svc} restarted`);
			}
		}
	},
});
