import { os, ORPCError } from "@orpc/server"
import { eq, and, desc } from "drizzle-orm"
import { parse } from "yaml"
import { db } from "@atlas/db"
import {
	organizations,
	servers,
	projects,
	deploys,
	domains,
	secrets,
	auditLog,
	users,
} from "@atlas/db/schema"
import { requireAuth, assertRole, type AuthContext } from "@atlas/auth"
import { encrypt, decrypt } from "@atlas/crypto"
import { KubernetesService } from "@atlas/kubernetes"
import { CloudflareClient } from "@atlas/cloudflare"
import { provisionServer } from "./services/provisioner"
import { executeDeploy } from "./services/deployer"

// ── Auth middleware ──

const authed = os.$context<{ request: Request }>().use(async ({ context, next }) => {
	const auth = await requireAuth(context.request.headers.get("authorization") ?? undefined)
	return next({ context: { auth } })
})

// ── Org ──

const org = {
	get: authed.handler(async ({ context }) => {
		const o = await db.query.organizations.findFirst({
			where: eq(organizations.id, context.auth.orgId),
		})
		if (!o) throw new ORPCError("NOT_FOUND")
		return {
			id: o.id,
			name: o.name,
			slug: o.slug,
			githubOrg: o.githubOrg,
			cloudflareConfigured: !!o.cloudflareTokenEnc,
			cloudflareAccountId: o.cloudflareAccountId,
			githubAppConfigured: !!o.githubClientId,
			githubAppId: o.githubAppId,
		}
	}),
	updateSettings: authed
		.use(({ context, next }) => {
			assertRole(context.auth, "admin")
			return next({})
		})
		.input(
			(await import("zod")).z.object({
				cloudflareToken: (await import("zod")).z.string().optional(),
				cloudflareAccountId: (await import("zod")).z.string().optional(),
				githubToken: (await import("zod")).z.string().optional(),
				githubAppId: (await import("zod")).z.number().optional(),
				githubClientId: (await import("zod")).z.string().optional(),
				githubClientSecret: (await import("zod")).z.string().optional(),
			}),
		)
		.handler(async ({ input, context }) => {
			const updates: Record<string, unknown> = {}

			if (input.cloudflareToken && input.cloudflareAccountId) {
				const cf = new CloudflareClient(input.cloudflareToken, input.cloudflareAccountId)
				if (!(await cf.verify())) throw new ORPCError("BAD_REQUEST", { message: "Invalid Cloudflare token" })
				updates.cloudflareTokenEnc = await encrypt(input.cloudflareToken)
				updates.cloudflareAccountId = input.cloudflareAccountId
			}
			if (input.githubToken) updates.githubTokenEnc = await encrypt(input.githubToken)
			if (input.githubAppId) updates.githubAppId = input.githubAppId
			if (input.githubClientId) updates.githubClientId = input.githubClientId
			if (input.githubClientSecret) updates.githubClientSecretEnc = await encrypt(input.githubClientSecret)

			await db.update(organizations).set(updates).where(eq(organizations.id, context.auth.orgId))

			await db.insert(auditLog).values({
				orgId: context.auth.orgId,
				userId: context.auth.userId,
				action: "org.settings.update",
				resourceType: "organization",
				resourceId: context.auth.orgId,
				meta: { fields: Object.keys(updates) },
			})

			return { ok: true as const }
		}),
}

// ── Servers ──

const serversProcedures = {
	list: authed.handler(async ({ context }) => {
		return db.select().from(servers).where(eq(servers.orgId, context.auth.orgId))
	}),
	get: authed
		.input((await import("zod")).z.object({ id: (await import("zod")).z.number() }))
		.handler(async ({ input, context }) => {
			const [server] = await db
				.select()
				.from(servers)
				.where(and(eq(servers.id, input.id), eq(servers.orgId, context.auth.orgId)))
				.limit(1)
			if (!server) throw new ORPCError("NOT_FOUND")
			return server
		}),
	create: authed
		.use(({ context, next }) => {
			assertRole(context.auth, "admin")
			return next({})
		})
		.input(
			(await import("zod")).z.object({
				name: (await import("zod")).z.string(),
				host: (await import("zod")).z.string(),
				ip: (await import("zod")).z.string().optional(),
				provision: (await import("zod")).z.boolean().optional(),
				domain: (await import("zod")).z.string().optional(),
			}),
		)
		.handler(async ({ input, context }) => {
			const [server] = await db
				.insert(servers)
				.values({
					name: input.name,
					host: input.host,
					ip: input.ip,
					status: input.provision ? "provisioning" : "offline",
					orgId: context.auth.orgId,
				})
				.returning()

			await db.insert(auditLog).values({
				orgId: context.auth.orgId,
				userId: context.auth.userId,
				action: "server.create",
				resourceType: "server",
				resourceId: server!.id,
			})

			if (input.provision && input.domain) {
				provisionServer({
					serverId: server!.id,
					host: input.host,
					domain: input.domain,
					orgId: context.auth.orgId,
				}).catch(console.error)
			}

			return server!
		}),
	delete: authed
		.use(({ context, next }) => {
			assertRole(context.auth, "admin")
			return next({})
		})
		.input((await import("zod")).z.object({ id: (await import("zod")).z.number() }))
		.handler(async ({ input, context }) => {
			const [deleted] = await db
				.delete(servers)
				.where(and(eq(servers.id, input.id), eq(servers.orgId, context.auth.orgId)))
				.returning()
			if (!deleted) throw new ORPCError("NOT_FOUND")
			return { ok: true as const }
		}),
}

// ── Projects ──

const projectsProcedures = {
	list: authed.handler(async ({ context }) => {
		return db.query.projects.findMany({
			where: eq(projects.orgId, context.auth.orgId),
			with: { server: true, domains: true },
		})
	}),
	get: authed
		.input((await import("zod")).z.object({ id: (await import("zod")).z.number() }))
		.handler(async ({ input, context }) => {
			const project = await db.query.projects.findFirst({
				where: and(eq(projects.id, input.id), eq(projects.orgId, context.auth.orgId)),
				with: {
					server: true,
					domains: true,
					deploys: { limit: 10, orderBy: (d, { desc }) => [desc(d.startedAt)] },
				},
			})
			if (!project) throw new ORPCError("NOT_FOUND")
			return project
		}),
	create: authed
		.use(({ context, next }) => {
			assertRole(context.auth, "admin")
			return next({})
		})
		.input(
			(await import("zod")).z.object({
				name: (await import("zod")).z.string(),
				serverId: (await import("zod")).z.number().optional(),
				githubRepo: (await import("zod")).z.string().optional(),
				domain: (await import("zod")).z.string().optional(),
				atlasYaml: (await import("zod")).z.string().optional(),
			}),
		)
		.handler(async ({ input, context }) => {
			const slug = input.name.toLowerCase().replace(/[^a-z0-9-]/g, "-")
			const [project] = await db
				.insert(projects)
				.values({
					name: input.name,
					slug,
					orgId: context.auth.orgId,
					serverId: input.serverId,
					githubRepo: input.githubRepo,
					domain: input.domain,
					atlasYaml: input.atlasYaml,
				})
				.returning()
			return project!
		}),
}

// ── Deploys ──

const deploysProcedures = {
	listByProject: authed
		.input((await import("zod")).z.object({ projectId: (await import("zod")).z.number() }))
		.handler(async ({ input, context }) => {
			const project = await db.query.projects.findFirst({
				where: and(eq(projects.id, input.projectId), eq(projects.orgId, context.auth.orgId)),
			})
			if (!project) return []
			return db
				.select()
				.from(deploys)
				.where(eq(deploys.projectId, project.id))
				.orderBy(desc(deploys.startedAt))
				.limit(50)
		}),
	trigger: authed
		.use(({ context, next }) => {
			assertRole(context.auth, "admin", "dev")
			return next({})
		})
		.input(
			(await import("zod")).z.object({
				projectId: (await import("zod")).z.number(),
				tag: (await import("zod")).z.string(),
				services: (await import("zod")).z.array((await import("zod")).z.string()).optional(),
			}),
		)
		.handler(async ({ input, context }) => {
			const project = await db.query.projects.findFirst({
				where: and(eq(projects.id, input.projectId), eq(projects.orgId, context.auth.orgId)),
				with: { server: true },
			})
			if (!project) throw new ORPCError("NOT_FOUND")
			if (!project.server) throw new ORPCError("BAD_REQUEST", { message: "No server assigned" })

			const [deploy] = await db
				.insert(deploys)
				.values({
					projectId: project.id,
					userId: context.auth.userId,
					tag: input.tag,
					status: "pending",
					meta: input.services ? { services: input.services } : undefined,
				})
				.returning()

			await db.insert(auditLog).values({
				orgId: context.auth.orgId,
				userId: context.auth.userId,
				action: "deploy.trigger",
				resourceType: "deploy",
				resourceId: deploy!.id,
				meta: { project: project.slug, tag: input.tag },
			})

			executeDeploy(deploy!.id).catch(console.error)
			return deploy!
		}),
	get: authed
		.input((await import("zod")).z.object({ id: (await import("zod")).z.number() }))
		.handler(async ({ input, context }) => {
			const [deploy] = await db.select().from(deploys).where(eq(deploys.id, input.id)).limit(1)
			if (!deploy) throw new ORPCError("NOT_FOUND")
			const project = await db.query.projects.findFirst({
				where: and(eq(projects.id, deploy.projectId), eq(projects.orgId, context.auth.orgId)),
			})
			if (!project) throw new ORPCError("NOT_FOUND")
			return deploy
		}),
}

// ── Secrets ──

const secretsProcedures = {
	listKeys: authed
		.input((await import("zod")).z.object({ projectId: (await import("zod")).z.number() }))
		.handler(async ({ input }) => {
			return db
				.select({ key: secrets.key, updatedAt: secrets.updatedAt })
				.from(secrets)
				.where(eq(secrets.projectId, input.projectId))
		}),
	set: authed
		.input(
			(await import("zod")).z.object({
				projectId: (await import("zod")).z.number(),
				secrets: (await import("zod")).z.record((await import("zod")).z.string(), (await import("zod")).z.string()),
			}),
		)
		.handler(async ({ input, context }) => {
			for (const [key, value] of Object.entries(input.secrets)) {
				const valueEnc = await encrypt(value)
				const existing = await db
					.select()
					.from(secrets)
					.where(and(eq(secrets.projectId, input.projectId), eq(secrets.key, key)))
					.limit(1)

				if (existing.length > 0) {
					await db.update(secrets).set({ valueEnc, updatedAt: new Date() }).where(eq(secrets.id, existing[0]!.id))
				} else {
					await db.insert(secrets).values({ projectId: input.projectId, key, valueEnc })
				}
			}

			// Sync to K8s
			const project = await db.query.projects.findFirst({
				where: eq(projects.id, input.projectId),
				with: { server: true },
			})
			let synced = false
			if (project?.server?.host) {
				const rows = await db.select().from(secrets).where(eq(secrets.projectId, input.projectId))
				const data: Record<string, string> = {}
				for (const row of rows) data[row.key] = await decrypt(row.valueEnc)
				const kube = new KubernetesService(project.server.host)
				synced = await kube.syncSecret(project.slug, `${project.slug}-secrets`, data)
			}

			return { ok: true as const, keys: Object.keys(input.secrets), synced }
		}),
	delete: authed
		.input(
			(await import("zod")).z.object({
				projectId: (await import("zod")).z.number(),
				key: (await import("zod")).z.string(),
			}),
		)
		.handler(async ({ input, context }) => {
			await db
				.delete(secrets)
				.where(and(eq(secrets.projectId, input.projectId), eq(secrets.key, input.key)))

			const project = await db.query.projects.findFirst({
				where: eq(projects.id, input.projectId),
				with: { server: true },
			})
			let synced = false
			if (project?.server?.host) {
				const kube = new KubernetesService(project.server.host)
				synced = await kube.deleteSecretKey(project.slug, `${project.slug}-secrets`, input.key)
			}

			return { ok: true as const, deleted: input.key, synced }
		}),
	pullValues: authed
		.input((await import("zod")).z.object({ projectId: (await import("zod")).z.number() }))
		.handler(async ({ input }) => {
			const rows = await db.select().from(secrets).where(eq(secrets.projectId, input.projectId))
			const result: Record<string, string> = {}
			for (const row of rows) result[row.key] = await decrypt(row.valueEnc)
			return result
		}),
}

// ── Router ──

export const router = {
	org,
	servers: serversProcedures,
	projects: projectsProcedures,
	deploys: deploysProcedures,
	secrets: secretsProcedures,
}

export type AppRouter = typeof router
