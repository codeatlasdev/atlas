import { defineCommand } from "citty"
import pc from "picocolors"
import { ssh } from "../lib/ssh"
import { loadConfig } from "../lib/config"

const migrate = defineCommand({
	meta: { name: "migrate", description: "Run database migrations" },
	args: {
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		console.log("→ Running migrations...")
		const result = await ssh(host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} exec deploy/server -- bun run db:push 2>&1`)

		console.log(result.stdout)
		if (!result.ok) console.error(result.stderr)
		else console.log("✓ Migrations complete")
	},
})

const studio = defineCommand({
	meta: { name: "studio", description: "Open Drizzle Studio (port-forward)" },
	args: {
		host: { type: "string", description: "SSH host" },
		port: { type: "string", description: "Local port", default: "4983" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const port = args.port || "4983"
		console.log(`→ Port-forwarding PostgreSQL to localhost:${port}...`)
		console.log(pc.dim("  Then run: bun run db:studio"))
		console.log(pc.dim("  Ctrl+C to stop\n"))

		const proc = Bun.spawn(
			["ssh", "-o", "StrictHostKeyChecking=accept-new", "-L", `${port}:localhost:5432`, host,
				`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} port-forward svc/postgres 5432:5432`],
			{ stdout: "inherit", stderr: "inherit" },
		)
		await proc.exited
	},
})

const psql = defineCommand({
	meta: { name: "psql", description: "Open PostgreSQL shell" },
	args: {
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const proc = Bun.spawn(
			["ssh", "-t", "-o", "StrictHostKeyChecking=accept-new", host,
				`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} exec -it statefulset/postgres -- psql -U ${ns}`],
			{ stdout: "inherit", stderr: "inherit", stdin: "inherit" },
		)
		process.exit(await proc.exited)
	},
})

const backup = defineCommand({
	meta: { name: "backup", description: "Create a PostgreSQL backup" },
	args: {
		host: { type: "string", description: "SSH host" },
		output: { type: "string", alias: "o", description: "Output file" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const ns = config.domain ? config.domain.split('.')[0] : 'app'
		const host = args.host || config.host
		if (!host) { console.error("No host. Run: atlas infra setup"); return }

		const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19)
		const outFile = args.output || `backup-${timestamp}.sql`

		console.log(`→ Creating backup...`)
		const result = await ssh(host,
			`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} exec statefulset/postgres -- pg_dump -U ${ns} --clean --if-exists ${ns}`)

		if (!result.ok) { console.error(result.stderr); return }

		await Bun.write(outFile, result.stdout)
		const sizeMB = (result.stdout.length / 1024 / 1024).toFixed(2)
		console.log(`✓ Backup saved: ${pc.cyan(outFile)} (${sizeMB}MB)`)
	},
})

export default defineCommand({
	meta: { name: "db", description: "Database management" },
	subCommands: { migrate, studio, psql, backup },
})
