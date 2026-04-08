import { eq } from "drizzle-orm"
import { ssh } from "@atlas/ssh"
import { getProvisionPhases } from "@atlas/provisioner"
import { db } from "@atlas/db"
import { servers, auditLog } from "@atlas/db/schema"

interface ProvisionOptions {
	serverId: number
	host: string
	domain: string
	orgId: number
	skipMonitoring?: boolean
	skipArgocd?: boolean
}

export async function provisionServer(opts: ProvisionOptions): Promise<void> {
	const { serverId, host, domain, orgId } = opts
	const log = (msg: string) => console.log(`[provision:${serverId}] ${msg}`)

	try {
		log("Testing SSH...")
		const test = await ssh(host, "echo ok")
		if (!test.ok) throw new Error(`SSH failed: ${test.stderr}`)

		const info = await ssh(
			host,
			"echo $(nproc) vCPU / $(free -h | awk '/Mem/{print $2}') RAM / $(df -h / | awk 'NR==2{print $4}') free",
		)
		log(info.stdout.trim())

		const ipResult = await ssh(
			host,
			"curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'",
		)
		const ip = ipResult.stdout.trim()

		await db.update(servers).set({ ip, status: "provisioning" }).where(eq(servers.id, serverId))

		const phases = getProvisionPhases({
			domain,
			skipMonitoring: opts.skipMonitoring,
			skipArgocd: opts.skipArgocd,
		})

		for (const phase of phases) {
			log(`${phase.name}...`)
			const result = await ssh(host, phase.script)
			if (!result.ok) throw new Error(`${phase.name} failed: ${result.stderr || result.stdout}`)
			log(`${phase.name} ✓`)
		}

		const kcResult = await ssh(host, "cat /etc/rancher/k3s/k3s.yaml")
		const kubeconfig = kcResult.stdout.replace(/127\.0\.0\.1/g, ip)

		const { encrypt } = await import("@atlas/crypto")
		const kubeconfigEnc = await encrypt(kubeconfig)

		await db
			.update(servers)
			.set({
				status: "online",
				ip,
				kubeconfigEnc,
				meta: { provisionedAt: new Date().toISOString(), info: info.stdout.trim() },
			})
			.where(eq(servers.id, serverId))

		await db.insert(auditLog).values({
			orgId,
			action: "server.provisioned",
			resourceType: "server",
			resourceId: serverId,
			meta: { ip },
		})

		log("Server online ✓")
	} catch (e) {
		const error = e instanceof Error ? e.message : String(e)
		log(`FAILED: ${error}`)
		await db.update(servers).set({ status: "error", meta: { error } }).where(eq(servers.id, serverId))
	}
}
