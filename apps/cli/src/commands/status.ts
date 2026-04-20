import { ssh } from "@atlas/ssh";
import { Box, Text, createCliRenderer } from "@opentui/core";
import { defineCommand } from "citty";
import { loadConfig } from "../lib/config";
import { type StatusData, getMockStatusData, parseStatusOutput } from "../lib/status";
import { resolveNamespace } from "../lib/project";
import { Divider, Header, theme } from "../ui";

export default defineCommand({
	meta: { name: "status", description: "Show cluster and application status" },
	args: {
		host: { type: "string", description: "SSH host" },
		demo: { type: "boolean", description: "Show demo data (no SSH required)", default: false },
	},
	async run({ args }) {
		const config = await loadConfig();
		const runtime = config.runtime || "k3s";
		const ns = await resolveNamespace(config.domain);
		const host = args.host || config.host;

		if (!args.demo && !host) {
			console.error("No host configured. Run: atlas infra setup (or use --demo)");
			return;
		}

		const renderer = await createCliRenderer();
		const root = Box({ width: "100%", flexDirection: "column", padding: 1, gap: 1 });
		root.add(Header("atlas status", args.demo ? "demo mode" : `${host} (${runtime})`));
		root.add(Divider());

		const loading = Text({ content: "  ◆ Fetching cluster status...", fg: theme.primary, id: "loading" });
		root.add(loading);
		renderer.root.add(root);

		renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
			if (key.ctrl && key.name === "c") {
				renderer.destroy();
				process.exit(0);
			}
		});

		// ── Fetch or mock data ──

		let data: StatusData;

		if (args.demo) {
			await new Promise((r) => setTimeout(r, 300)); // simulate latency
			data = getMockStatusData(runtime);
		} else {
			const script = runtime === "swarm" ? swarmScript(ns) : k3sScript(ns);
			const result = await ssh(host!, script);
			root.remove("loading");

			if (!result.ok) {
				root.add(Text({ content: `  ✗ Failed: ${result.stderr}`, fg: theme.error }));
				renderer.requestRender();
				await new Promise((r) => setTimeout(r, 2000));
				renderer.destroy();
				return;
			}

			data = parseStatusOutput(result.stdout, runtime);
		}

		root.remove("loading");

		// ── Render ──

		renderStatusData(root, data, runtime, ns);

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

// ── Rendering ──

function renderStatusData(
	root: ReturnType<typeof Box>,
	data: StatusData,
	runtime: string,
	ns: string,
) {
	const sections: { title: string; key: keyof StatusData }[] = [
		{ title: "⚙️  Runtime", key: "runtime" },
		{ title: runtime === "swarm" ? "🐳 Nodes" : "☸ Cluster", key: "nodes" },
		{ title: `🚀 Application (${ns})`, key: "appServices" },
		{ title: "⚙️  Infrastructure", key: "infraServices" },
		{ title: "💾 Memory", key: "memory" },
	];

	for (const { title, key } of sections) {
		root.add(Text({ content: "", fg: theme.border }));
		root.add(Text({ content: `  ${title}`, fg: theme.text }));

		if (key === "runtime") {
			root.add(Text({ content: `    ${data.runtime}`, fg: theme.primary }));
		} else if (key === "nodes") {
			for (const node of data.nodes) {
				root.add(Text({ content: `    ${node}`, fg: theme.muted }));
			}
		} else if (key === "appServices") {
			for (const svc of data.appServices) {
				const color =
					svc.status === "Running" || svc.status.includes("/")
						? theme.success
						: svc.status === "Pending"
							? theme.warning
							: theme.error;
				const extra = svc.extra ? ` ${svc.extra}` : "";
				root.add(Text({ content: `    ● ${svc.name} ${svc.status}${extra}`, fg: color }));
			}
		} else if (key === "infraServices") {
			for (const svc of data.infraServices) {
				root.add(
					Text({
						content: `    ● ${svc.name.padEnd(20)} ${svc.counts}`,
						fg: svc.healthy ? theme.success : theme.warning,
					}),
				);
			}
		} else if (key === "memory") {
			root.add(Text({ content: `    ${data.memory}`, fg: theme.text }));
		}
	}
}

// ── SSH scripts ──

function k3sScript(ns: string) {
	return `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

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

function swarmScript(ns: string) {
	return `echo "RUNTIME"
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
}
