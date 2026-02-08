import { Elysia, t } from "elysia"
import { eq, and } from "drizzle-orm"
import { db } from "../db"
import { secrets, projects, servers, auditLog } from "../db/schema"
import { requireAuth } from "../lib/guard"
import { encrypt, decrypt } from "../lib/crypto"
import { KubernetesService } from "../services/kubernetes"

async function syncToCluster(projectId: number): Promise<boolean> {
	const project = await db.query.projects.findFirst({
		where: eq(projects.id, projectId),
		with: { server: true },
	})
	if (!project?.server?.host) return false

	const rows = await db.select().from(secrets).where(eq(secrets.projectId, projectId))
	const data: Record<string, string> = {}
	for (const row of rows) {
		data[row.key] = await decrypt(row.valueEnc)
	}

	const ns = project.slug
	const kube = new KubernetesService(project.server.host)
	try {
		return await kube.syncSecret(ns, `${ns}-secrets`, data)
	} finally {
		await kube.cleanup()
	}
}

export const secretsRoutes = new Elysia({ prefix: "/secrets" })
	// List keys (no values)
	.get(
		"/project/:projectId",
		async ({ headers, params }) => {
			const auth = await requireAuth(headers.authorization)
			const rows = await db
				.select({ key: secrets.key, updatedAt: secrets.updatedAt })
				.from(secrets)
				.where(eq(secrets.projectId, Number(params.projectId)))
			return rows
		},
	)
	// Set secrets (bulk upsert)
	.put(
		"/project/:projectId",
		async ({ headers, params, body }) => {
			const auth = await requireAuth(headers.authorization)
			const projectId = Number(params.projectId)

			for (const [key, value] of Object.entries(body.secrets)) {
				const valueEnc = await encrypt(value)
				const existing = await db
					.select()
					.from(secrets)
					.where(and(eq(secrets.projectId, projectId), eq(secrets.key, key)))
					.limit(1)

				if (existing.length > 0) {
					await db
						.update(secrets)
						.set({ valueEnc, updatedAt: new Date() })
						.where(eq(secrets.id, existing[0].id))
				} else {
					await db.insert(secrets).values({ projectId, key, valueEnc })
				}
			}

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "secrets.set",
				resourceType: "project",
				resourceId: projectId,
				meta: { keys: Object.keys(body.secrets) },
			})

			// Sync to K8s
			const synced = await syncToCluster(projectId)

			return { ok: true, keys: Object.keys(body.secrets), synced }
		},
		{
			body: t.Object({
				secrets: t.Record(t.String(), t.String()),
			}),
		},
	)
	// Delete a key
	.delete(
		"/project/:projectId/:key",
		async ({ headers, params }) => {
			const auth = await requireAuth(headers.authorization)
			const projectId = Number(params.projectId)

			await db
				.delete(secrets)
				.where(and(eq(secrets.projectId, projectId), eq(secrets.key, params.key)))

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "secrets.delete",
				resourceType: "project",
				resourceId: projectId,
				meta: { key: params.key },
			})

			// Remove key from K8s Secret directly
			const project = await db.query.projects.findFirst({
				where: eq(projects.id, projectId),
				with: { server: true },
			})
			let synced = false
			if (project?.server?.host) {
				const kube = new KubernetesService(project.server.host)
				try {
					synced = await kube.deleteSecretKey(project.slug, `${project.slug}-secrets`, params.key)
				} finally {
					await kube.cleanup()
				}
			}

			return { ok: true, deleted: params.key, synced }
		},
	)
	// Pull secrets (returns decrypted values — CLI only)
	.get(
		"/project/:projectId/values",
		async ({ headers, params }) => {
			const auth = await requireAuth(headers.authorization)
			const rows = await db
				.select()
				.from(secrets)
				.where(eq(secrets.projectId, Number(params.projectId)))

			const result: Record<string, string> = {}
			for (const row of rows) {
				result[row.key] = await decrypt(row.valueEnc)
			}
			return result
		},
	)
