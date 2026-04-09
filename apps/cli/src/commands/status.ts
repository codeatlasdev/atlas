import { ssh } from "@atlas/ssh";
import { Box, Text, createCliRenderer } from "@opentui/core";
import { defineCommand } from "citty";
import { loadConfig } from "../lib/config";
import { Divider, Header, theme } from "../ui";

export default defineCommand({
	meta: { name: "status", description: "Show cluster and application status" },
	args: {
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig();
		const ns = config.domain ? config.domain.split(".")[0] : "app";
		const host = args.host || config.host;
		const runtime = config.runtime || "k3s";
		if (!host) {
			console.error("No host configured. Run: atlas infra setup");
			return;
		}

		const renderer = await createCliRenderer();
		const root = Box({ width: "100%", flexDirection: "column", padding: 1, gap: 1 });
		root.add(Header("atlas status", `${host} (${runtime})`));
		root.add(Divider());

		const loading = Text({ content: "  ◆ Fetching cluster status...", fg: theme.primary });
		root.add(loading);
		renderer.root.add(root);

		renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
			if (key.ctrl && key.name === "c") {
				renderer.destroy();
				process.exit(0);
			}
		});

		let script: string;
		if (runtime === "swarm") {
			script = `echo "RUNTIME"
echo "Docker Swarm"

echo "NODE"
docker node ls --format "table {{.Hostname}}\t{{.Status}}\t{{.Availability}}\t{{.ManagerStatus}}" 2>/dev/null || echo "(not a swarm manager)"

echo "APP_SERVICES"
docker service ls --filter "label=com.docker.stack.namespace=${ns}" --format "{{.Name}} {{.Replicas}} {{.Image}}" 2>/dev/null || echo "(no services)"

echo "INFRA_SERVICES"
for stack in traefik monitoring; do
  running=$(docker service ls --filter "label=com.docker.stack.namespace=$stack" --format "{{.Replicas}}" 2>/dev/null | awk -F/ '{s+=$1;t+=$2}END{printf "%d/%d",s,t}')
  echo "$stack $running"
done

echo "MEMORY"
free -m | awk '/Mem/{printf "%s/%sMB (%.0f%%)", $3, $2, $3/$2*100}'`;
		} else {
			script = `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

echo "RUNTIME"
echo "K3s $(k3s --version 2>/dev/null | head -1 | awk '{print $3}')"

echo "NODE"
kubectl get nodes -o custom-columns='NAME:.metadata.name,VERSION:.status.nodeInfo.kubeletVersion,CPU:.status.capacity.cpu,MEM:.status.capacity.memory' --no-headers

echo "APP_SERVICES"
kubectl -n ${ns} get pods --no-headers 2>/dev/null | awk '{printf "%s %s %s %s\\n", $1, $3, $4, $5}' || echo "(no pods)"

echo "INFRA_SERVICES"
for n in kube-system cert-manager monitoring argocd; do
  running=$(kubectl -n $n get pods --no-headers 2>/dev/null | grep -c Running || echo 0)
  total=$(kubectl -n $n get pods --no-headers 2>/dev/null | wc -l)
  echo "$n $running/$total"
done

echo "MEMORY"
free -m | awk '/Mem/{printf "%s/%sMB (%.0f%%)", $3, $2, $3/$2*100}'`;
		}

		const result = await ssh(host, script);
		root.remove(loading);

		if (!result.ok) {
			root.add(Text({ content: `  ✗ Failed: ${result.stderr}`, fg: theme.error }));
			renderer.requestRender();
			await new Promise((r) => setTimeout(r, 2000));
			renderer.destroy();
			return;
		}

		const lines = result.stdout.split("\n");
		let section = "";
		const sectionKey = runtime === "swarm" ? "APP_SERVICES" : "APP_SERVICES";

		for (const line of lines) {
			const trimmed = line.trim();
			if (["RUNTIME", "NODE", "APP_SERVICES", "INFRA_SERVICES", "MEMORY"].includes(trimmed)) {
				section = trimmed;
				const titles: Record<string, string> = {
					RUNTIME: "⚙️  Runtime",
					NODE: runtime === "swarm" ? "🐳 Nodes" : "☸ Cluster",
					APP_SERVICES: `🚀 Application (${ns})`,
					INFRA_SERVICES: "⚙️  Infrastructure",
					MEMORY: "💾 Memory",
				};
				root.add(Text({ content: "", fg: theme.border }));
				root.add(Text({ content: `  ${titles[section]}`, fg: theme.text, bold: true }));
				continue;
			}
			if (!trimmed) continue;

			if (section === "RUNTIME") {
				root.add(Text({ content: `    ${trimmed}`, fg: theme.primary }));
			} else if (section === "APP_SERVICES") {
				const parts = trimmed.split(/\s+/);
				const name = parts[0] || "";
				const status = parts[1] || "";
				const color =
					status === "Running" || status.includes("/")
						? theme.success
						: status === "Pending"
							? theme.warning
							: theme.error;
				root.add(Text({ content: `    ● ${name} ${status}`, fg: color }));
			} else if (section === "INFRA_SERVICES") {
				const [nsName, counts] = trimmed.split(/\s+/);
				const ok = counts && counts.split("/")[0] === counts.split("/")[1];
				root.add(
					Text({
						content: `    ● ${(nsName ?? "").padEnd(20)} ${counts}`,
						fg: ok ? theme.success : theme.warning,
					}),
				);
			} else if (section === "MEMORY") {
				root.add(Text({ content: `    ${trimmed}`, fg: theme.text }));
			} else {
				root.add(Text({ content: `    ${trimmed}`, fg: theme.muted }));
			}
		}

		root.add(Text({ content: "", fg: theme.border }));
		root.add(Text({ content: "  Press q to exit", fg: theme.muted }));
		renderer.requestRender();

		await new Promise<void>((resolve) => {
			renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
				if (key.name === "q" || (key.ctrl && key.name === "c")) resolve();
			});
		});
		renderer.destroy();
	},
});
