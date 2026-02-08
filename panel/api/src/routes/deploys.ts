import { Elysia, t } from "elysia"
import { eq, and, desc } from "drizzle-orm"
import { db } from "../db"
import { deploys, projects, auditLog } from "../db/schema"
import { requireAuth, assertRole } from "../lib/guard"
import { executeDeploy } from "../services/deployer"

export const deployRoutes = new Elysia({ prefix: "/deploys" })
	.get("/project/:projectId", async ({ headers, params }) => {
		const auth = await requireAuth(headers.authorization)
		const project = await db.query.projects.findFirst({
			where: and(eq(projects.id, Number(params.projectId)), eq(projects.orgId, auth.orgId)),
		})
		if (!project) return []

		return db
			.select()
			.from(deploys)
			.where(eq(deploys.projectId, project.id))
			.orderBy(desc(deploys.startedAt))
			.limit(50)
	}, { params: t.Object({ projectId: t.String() }) })
	.post(
		"/project/:projectId",
		async ({ headers, params, body, set }) => {
			const auth = await requireAuth(headers.authorization)
			assertRole(auth, "admin", "dev")

			const project = await db.query.projects.findFirst({
				where: and(eq(projects.id, Number(params.projectId)), eq(projects.orgId, auth.orgId)),
				with: { server: true },
			})

			if (!project) {
				set.status = 404
				return { error: "Project not found" }
			}

			if (!project.server) {
				set.status = 400
				return { error: "Project has no server assigned" }
			}

			const [deploy] = await db
				.insert(deploys)
				.values({
					projectId: project.id,
					userId: auth.userId,
					tag: body.tag,
					status: "pending",
					meta: body.services ? { services: body.services } : undefined,
				})
				.returning()

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "deploy.trigger",
				resourceType: "deploy",
				resourceId: deploy.id,
				meta: { project: project.slug, tag: body.tag },
			})

			// Execute deploy asynchronously
			executeDeploy(deploy.id).catch((e) => console.error("Deploy pipeline error:", e))

			return deploy
		},
		{
			params: t.Object({ projectId: t.String() }),
			body: t.Object({
				tag: t.String(),
				services: t.Optional(t.Array(t.String())),
			}),
		},
	)
	.get("/:id", async ({ headers, params, set }) => {
		const auth = await requireAuth(headers.authorization)

		const [deploy] = await db
			.select()
			.from(deploys)
			.where(eq(deploys.id, Number(params.id)))
			.limit(1)

		if (!deploy) {
			set.status = 404
			return { error: "Deploy not found" }
		}

		// Verify project belongs to org
		const project = await db.query.projects.findFirst({
			where: and(eq(projects.id, deploy.projectId), eq(projects.orgId, auth.orgId)),
		})
		if (!project) {
			set.status = 404
			return { error: "Deploy not found" }
		}

		return deploy
	}, { params: t.Object({ id: t.String() }) })
