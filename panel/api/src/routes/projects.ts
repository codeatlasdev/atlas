import { Elysia, t } from "elysia"
import { eq, and } from "drizzle-orm"
import { parse } from "yaml"
import { db } from "../db"
import { projects, domains, auditLog } from "../db/schema"
import { requireAuth, assertRole } from "../lib/guard"

async function syncDomains(projectId: number, atlasYaml: string, projectDomain?: string | null) {
	const config = parse(atlasYaml) as { domain?: string; services?: Record<string, { domain?: string }> }
	const hostnames = new Set<string>()

	if (projectDomain) hostnames.add(projectDomain)
	if (config.domain) hostnames.add(config.domain)
	if (config.services) {
		for (const svc of Object.values(config.services)) {
			if (svc.domain) hostnames.add(svc.domain)
		}
	}

	for (const hostname of hostnames) {
		await db.insert(domains).values({ projectId, hostname }).onConflictDoNothing()
	}
}

export const projectRoutes = new Elysia({ prefix: "/projects" })
	.get("/", async ({ headers }) => {
		const auth = await requireAuth(headers.authorization)
		return db.query.projects.findMany({
			where: eq(projects.orgId, auth.orgId),
			with: { server: true, domains: true },
		})
	})
	.get("/:id", async ({ headers, params, set }) => {
		const auth = await requireAuth(headers.authorization)
		const project = await db.query.projects.findFirst({
			where: and(eq(projects.id, Number(params.id)), eq(projects.orgId, auth.orgId)),
			with: { server: true, domains: true, deploys: { limit: 10, orderBy: (d, { desc }) => [desc(d.startedAt)] } },
		})

		if (!project) {
			set.status = 404
			return { error: "Project not found" }
		}
		return project
	}, { params: t.Object({ id: t.String() }) })
	.post(
		"/",
		async ({ headers, body, set }) => {
			const auth = await requireAuth(headers.authorization)
			assertRole(auth, "admin")

			const slug = body.name.toLowerCase().replace(/[^a-z0-9-]/g, "-")

			const [project] = await db
				.insert(projects)
				.values({
					name: body.name,
					slug,
					orgId: auth.orgId,
					serverId: body.serverId,
					githubRepo: body.githubRepo,
					domain: body.domain,
					atlasYaml: body.atlasYaml,
				})
				.returning()

			if (body.atlasYaml) {
				await syncDomains(project.id, body.atlasYaml, body.domain)
			} else if (body.domain) {
				await db.insert(domains).values({ projectId: project.id, hostname: body.domain }).onConflictDoNothing()
			}

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "project.create",
				resourceType: "project",
				resourceId: project.id,
				meta: { name: body.name, domain: body.domain },
			})

			return project
		},
		{
			body: t.Object({
				name: t.String(),
				serverId: t.Optional(t.Number()),
				githubRepo: t.Optional(t.String()),
				domain: t.Optional(t.String()),
				atlasYaml: t.Optional(t.String()),
			}),
		},
	)
	.put(
		"/:id",
		async ({ headers, params, body, set }) => {
			const auth = await requireAuth(headers.authorization)
			assertRole(auth, "admin")

			const [updated] = await db
				.update(projects)
				.set({
					serverId: body.serverId,
					domain: body.domain,
					atlasYaml: body.atlasYaml,
				})
				.where(and(eq(projects.id, Number(params.id)), eq(projects.orgId, auth.orgId)))
				.returning()

			if (!updated) {
				set.status = 404
				return { error: "Project not found" }
			}

			if (body.atlasYaml) {
				await syncDomains(updated.id, body.atlasYaml, body.domain ?? updated.domain)
			}

			return updated
		},
		{
			params: t.Object({ id: t.String() }),
			body: t.Object({
				serverId: t.Optional(t.Number()),
				domain: t.Optional(t.String()),
				atlasYaml: t.Optional(t.String()),
			}),
		},
	)
