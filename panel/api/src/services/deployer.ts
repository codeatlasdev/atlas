import { eq } from "drizzle-orm"
import { parse } from "yaml"
import { db } from "../db"
import { deploys, projects, servers, domains, organizations } from "../db/schema"
import { KubernetesService } from "./kubernetes"
import { CloudflareService } from "./cloudflare"

interface ServiceConfig {
	type: "api" | "web" | "worker"
	dockerfile: string
	target?: string
	port?: number
	domain?: string
}

interface ProjectConfig {
	name: string
	org: string
	domain?: string
	services: Record<string, ServiceConfig>
	infra?: { postgres?: boolean; redis?: boolean; tunnel?: boolean }
}

export async function executeDeploy(deployId: number): Promise<void> {
	const deploy = await db.query.deploys.findFirst({
		where: eq(deploys.id, deployId),
		with: { project: { with: { server: true } } },
	})

	if (!deploy?.project?.server) {
		await updateStatus(deployId, "failed", { error: "No server assigned" })
		return
	}

	const project = deploy.project
	const server = deploy.project.server!

	if (!server.host) {
		await updateStatus(deployId, "failed", { error: "Server has no host" })
		return
	}

	// Parse atlas.yaml
	let config: ProjectConfig
	if (project.atlasYaml) {
		config = parse(project.atlasYaml) as ProjectConfig
	} else {
		await updateStatus(deployId, "failed", { error: "Project has no atlas.yaml" })
		return
	}

	const registry = `ghcr.io/${config.org}/${config.name}`
	const ns = config.name
	const tag = deploy.tag
	const kube = new KubernetesService(server.host)

	try {
		// ── Deploy ──
		await updateStatus(deployId, "deploying")

		// Filter services to deploy (from meta or all)
		const meta = deploy.meta as { services?: string[] } | null
		let serviceEntries = Object.entries(config.services)
		if (meta?.services?.length) {
			serviceEntries = serviceEntries.filter(([name]) => meta.services!.includes(name))
		}

		// Run migration first if exists
		const hasMigrate = serviceEntries.some(([name]) => name === "migrate")
		if (hasMigrate) {
			const migrateImage = `${registry}/migrate:${tag}`
			const jobYaml = `apiVersion: batch/v1
kind: Job
metadata:
  name: migrate
spec:
  backoffLimit: 3
  ttlSecondsAfterFinished: 300
  template:
    spec:
      restartPolicy: OnFailure
      imagePullSecrets:
        - name: ghcr-auth
      containers:
        - name: migrate
          image: ${migrateImage}
          envFrom:
            - secretRef:
                name: ${ns}-secrets`

			await kube.deleteResource(ns, "job", "migrate")
			const { ok, stderr } = await kube.applyStdin(ns, jobYaml)
			if (!ok) console.error("Migration apply failed:", stderr)
		}

		// Set image for each service (except migrate)
		const deployable = serviceEntries.filter(([name]) => name !== "migrate")
		for (const [name] of deployable) {
			const image = `${registry}/${name}:${tag}`
			await kube.setImage(ns, name, name, image)
		}

		// Wait for rollout
		let allHealthy = true
		for (const [name] of deployable) {
			const ok = await kube.rolloutStatus(ns, name)
			if (!ok) {
				allHealthy = false
				console.error(`Rollout failed for ${name}`)
			}
		}

		if (!allHealthy) {
			await updateStatus(deployId, "failed", { error: "Rollout failed" })
			await kube.cleanup()
			return
		}

		// ── DNS ──
		const org = await db.query.organizations.findFirst({
			where: eq(organizations.id, project.orgId),
		})

		if (org?.cloudflareTokenEnc && org?.cloudflareAccountId && server.ip) {
			const cf = new CloudflareService(org.cloudflareTokenEnc, org.cloudflareAccountId)
			const domainsToSetup = serviceEntries
				.filter(([, svc]) => svc.domain)
				.map(([, svc]) => svc.domain!)

			if (config.domain && !domainsToSetup.includes(config.domain)) {
				domainsToSetup.push(config.domain)
			}

			for (const hostname of domainsToSetup) {
				try {
					const result = await cf.ensureDNS(hostname, server.ip)
					// Upsert domain with DNS record ID
					const existing = await db.select().from(domains)
						.where(eq(domains.hostname, hostname)).limit(1)

					if (existing.length > 0) {
						await db.update(domains)
							.set({ dnsRecordId: result.recordId, verified: true })
							.where(eq(domains.hostname, hostname))
					} else {
						await db.insert(domains).values({
							projectId: project.id, hostname,
							dnsRecordId: result.recordId, verified: true,
						})
					}

					if (result.action !== "exists") {
						console.log(`DNS: ${hostname} → ${result.action}`)
					}
				} catch (e) {
					console.error(`DNS failed for ${hostname}:`, e)
				}
			}
		}

		await updateStatus(deployId, "success")
	} catch (e) {
		const error = e instanceof Error ? e.message : String(e)
		await updateStatus(deployId, "failed", { error })
		console.error("Deploy failed:", error)
	} finally {
		await kube.cleanup()
	}
}

async function updateStatus(
	deployId: number,
	status: "pending" | "building" | "pushing" | "deploying" | "success" | "failed" | "rolled_back",
	meta?: Record<string, unknown>,
) {
	await db
		.update(deploys)
		.set({
			status,
			...(meta ? { meta } : {}),
			...(status === "success" || status === "failed" ? { finishedAt: new Date() } : {}),
		})
		.where(eq(deploys.id, deployId))
}
