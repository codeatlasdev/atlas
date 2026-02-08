import { defineCommand } from "citty"
import pc from "picocolors"
import { ssh } from "../lib/ssh"
import { loadConfig } from "../lib/config"

export default defineCommand({
	meta: {
		name: "status",
		description: "Show cluster and application status",
	},
	args: {
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

		const result = await ssh(
			host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

echo "NODE"
kubectl get nodes -o custom-columns='NAME:.metadata.name,VERSION:.status.nodeInfo.kubeletVersion,CPU:.status.capacity.cpu,MEM:.status.capacity.memory' --no-headers

echo ""
echo "RESOURCES"
kubectl top nodes --no-headers 2>/dev/null || echo "(metrics loading...)"

echo ""
echo "APP_PODS"
kubectl -n ${ns} get pods --no-headers 2>/dev/null | awk '{printf "%s %s %s %s\\n", $1, $3, $4, $5}' || echo "(no app pods)"

echo ""
echo "INFRA_PODS"
for ns in kube-system cert-manager monitoring argocd; do
  running=$(kubectl -n $ns get pods --no-headers 2>/dev/null | grep -c Running || echo 0)
  total=$(kubectl -n $ns get pods --no-headers 2>/dev/null | wc -l)
  echo "$ns $running/$total"
done

echo ""
echo "HELM"
helm list -A --no-headers 2>/dev/null | awk '{printf "%s %s %s %s\\n", $1, $2, $7, $9}'

echo ""
echo "MEMORY"
free -m | awk '/Mem/{printf "%s/%sMB (%.0f%%)", $3, $2, $3/$2*100}'`,
		)

		if (!result.ok) {
			console.error(`Failed: ${result.stderr}`)
			return
		}

		const lines = result.stdout.split("\n")
		let section = ""

		for (const line of lines) {
			if (line === "NODE") {
				section = "node"
				console.log(pc.bold("\n☸ Cluster"))
				continue
			}
			if (line === "RESOURCES") {
				section = "resources"
				console.log(pc.bold("\n📊 Resources"))
				continue
			}
			if (line === "APP_PODS") {
				section = "app"
				console.log(pc.bold(`\n🚀 Application (${ns})`))
				continue
			}
			if (line === "INFRA_PODS") {
				section = "infra"
				console.log(pc.bold("\n⚙️  Infrastructure"))
				continue
			}
			if (line === "HELM") {
				section = "helm"
				console.log(pc.bold("\n📦 Helm Releases"))
				continue
			}
			if (line === "MEMORY") {
				section = "memory"
				continue
			}
			if (!line.trim()) continue

			if (section === "app") {
				const [name, status, restarts] = line.split(/\s+/)
				const color = status === "Running" ? pc.green : status === "Pending" ? pc.yellow : pc.red
				console.log(`  ${color("●")} ${name} ${color(status)} ${restarts !== "0" ? pc.yellow(`(${restarts} restarts)`) : ""}`)
			} else if (section === "infra") {
				const [ns, counts] = line.split(/\s+/)
				const ok = counts && counts.split("/")[0] === counts.split("/")[1]
				console.log(`  ${ok ? pc.green("●") : pc.yellow("●")} ${ns.padEnd(20)} ${counts}`)
			} else if (section === "memory") {
				console.log(pc.bold(`\n💾 Memory: ${line}`))
			} else {
				console.log(`  ${line}`)
			}
		}
		console.log()
	},
})
