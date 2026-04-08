import { defineCommand } from "citty"
import { createCliRenderer, Box, Text } from "@opentui/core"
import { ssh } from "@atlas/ssh"
import { loadConfig } from "../lib/config"
import { theme, Header, Divider } from "../ui"

export default defineCommand({
	meta: { name: "status", description: "Show cluster and application status" },
	args: {
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split(".")[0] : "app"
		const host = args.host || config.host
		if (!host) {
			console.error("No host configured. Run: atlas infra setup")
			return
		}

		const renderer = await createCliRenderer()

		const root = Box({ width: "100%", flexDirection: "column", padding: 1, gap: 1 })
		root.add(Header("atlas status", host))
		root.add(Divider())

		const loading = Text({ content: "  ◆ Fetching cluster status...", fg: theme.primary })
		root.add(loading)
		renderer.root.add(root)

		// Handle Ctrl+C
		renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
			if (key.ctrl && key.name === "c") {
				renderer.destroy()
				process.exit(0)
			}
		})

		const result = await ssh(
			host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

echo "NODE"
kubectl get nodes -o custom-columns='NAME:.metadata.name,VERSION:.status.nodeInfo.kubeletVersion,CPU:.status.capacity.cpu,MEM:.status.capacity.memory' --no-headers

echo "APP_PODS"
kubectl -n ${ns} get pods --no-headers 2>/dev/null | awk '{printf "%s %s %s %s\\n", $1, $3, $4, $5}' || echo "(no pods)"

echo "INFRA_PODS"
for n in kube-system cert-manager monitoring argocd; do
  running=$(kubectl -n $n get pods --no-headers 2>/dev/null | grep -c Running || echo 0)
  total=$(kubectl -n $n get pods --no-headers 2>/dev/null | wc -l)
  echo "$n $running/$total"
done

echo "MEMORY"
free -m | awk '/Mem/{printf "%s/%sMB (%.0f%%)", $3, $2, $3/$2*100}'`,
		)

		root.remove(loading)

		if (!result.ok) {
			root.add(Text({ content: `  ✗ Failed: ${result.stderr}`, fg: theme.error }))
			renderer.requestRender()
			await new Promise((r) => setTimeout(r, 2000))
			renderer.destroy()
			return
		}

		const lines = result.stdout.split("\n")
		let section = ""

		for (const line of lines) {
			if (["NODE", "APP_PODS", "INFRA_PODS", "MEMORY"].includes(line.trim())) {
				section = line.trim()
				const titles: Record<string, string> = {
					NODE: "☸ Cluster",
					APP_PODS: `🚀 Application (${ns})`,
					INFRA_PODS: "⚙️  Infrastructure",
					MEMORY: "💾 Memory",
				}
				root.add(Text({ content: "", fg: theme.border }))
				root.add(Text({ content: `  ${titles[section]}`, fg: theme.text, bold: true }))
				continue
			}
			if (!line.trim()) continue

			if (section === "APP_PODS") {
				const [name, status, restarts] = line.split(/\s+/)
				const color = status === "Running" ? theme.success : status === "Pending" ? theme.warning : theme.error
				const restart = restarts && restarts !== "0" ? ` (${restarts} restarts)` : ""
				root.add(Text({ content: `    ● ${name} ${status}${restart}`, fg: color }))
			} else if (section === "INFRA_PODS") {
				const [nsName, counts] = line.split(/\s+/)
				const ok = counts && counts.split("/")[0] === counts.split("/")[1]
				root.add(
					Text({
						content: `    ● ${(nsName ?? "").padEnd(20)} ${counts}`,
						fg: ok ? theme.success : theme.warning,
					}),
				)
			} else if (section === "MEMORY") {
				root.add(Text({ content: `    ${line}`, fg: theme.text }))
			} else {
				root.add(Text({ content: `    ${line}`, fg: theme.muted }))
			}
		}

		root.add(Text({ content: "", fg: theme.border }))
		root.add(Text({ content: "  Press q to exit", fg: theme.muted }))
		renderer.requestRender()

		// Wait for q or Ctrl+C
		await new Promise<void>((resolve) => {
			renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
				if (key.name === "q" || (key.ctrl && key.name === "c")) {
					resolve()
				}
			})
		})

		renderer.destroy()
	},
})
