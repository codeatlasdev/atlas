import { defineCommand } from "citty"
import pc from "picocolors"
import { ssh } from "../lib/ssh"
import { loadConfig } from "../lib/config"

const list = defineCommand({
	meta: { name: "list", description: "List environment variables" },
	args: {
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const result = await ssh(host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} get secret ${ns}-secrets -o json 2>/dev/null | jq -r '.data | to_entries[] | "\\(.key)=\\(.value)"'`)

		if (!result.ok) { console.error(result.stderr); return }

		console.log(pc.bold(`\n🔐 Environment variables (${ns}-secrets)\n`))
		for (const line of result.stdout.trim().split("\n")) {
			if (!line) continue
			const [key, val] = line.split("=", 2)
			const decoded = Buffer.from(val, "base64").toString()
			// Mask sensitive values
			const masked = decoded.length > 8 ? `${decoded.slice(0, 4)}${"*".repeat(decoded.length - 4)}` : "****"
			console.log(`  ${pc.cyan(key)}=${masked}`)
		}
		console.log()
	},
})

const set = defineCommand({
	meta: { name: "set", description: "Set an environment variable" },
	args: {
		keyvalue: { type: "positional", description: "KEY=value", required: true },
		host: { type: "string", description: "SSH host" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const kv = args.keyvalue as string
		const eqIdx = kv.indexOf("=")
		if (eqIdx === -1) { console.error("Format: atlas env set KEY=value"); return }

		const key = kv.slice(0, eqIdx)
		const value = kv.slice(eqIdx + 1)

		const result = await ssh(host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} get secret ${ns}-secrets -o json | \
  jq --arg k "${key}" --arg v "$(echo -n '${value}' | base64)" '.data[$k] = $v' | \
  kubectl apply -f -`)

		if (!result.ok) { console.error(result.stderr); return }
		console.log(`✓ ${pc.cyan(key)} updated. Restart pods: atlas restart server`)
	},
})

const pull = defineCommand({
	meta: { name: "pull", description: "Download .env file from cluster" },
	args: {
		host: { type: "string", description: "SSH host" },
		output: { type: "string", alias: "o", description: "Output file", default: ".env.cluster" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const result = await ssh(host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} get secret ${ns}-secrets -o json | jq -r '.data | to_entries[] | "\\(.key)=\\(.value | @base64d)"'`)

		if (!result.ok) { console.error(result.stderr); return }

		const outFile = args.output || ".env.cluster"
		await Bun.write(outFile, result.stdout)
		console.log(`✓ Saved to ${pc.cyan(outFile)}`)
	},
})

export default defineCommand({
	meta: { name: "env", description: "Manage environment variables" },
	subCommands: { list, set, pull },
})
