import { Elysia, t } from "elysia"
import { eq, and } from "drizzle-orm"
import { db } from "../db"
import { servers, auditLog } from "../db/schema"
import { requireAuth, assertRole } from "../lib/guard"
import { provisionServer } from "../services/provisioner"

export const serverRoutes = new Elysia({ prefix: "/servers" })
	.get("/", async ({ headers }) => {
		const auth = await requireAuth(headers.authorization)
		return db.select().from(servers).where(eq(servers.orgId, auth.orgId))
	})
	.get("/:id", async ({ headers, params, set }) => {
		const auth = await requireAuth(headers.authorization)
		const [server] = await db
			.select()
			.from(servers)
			.where(and(eq(servers.id, Number(params.id)), eq(servers.orgId, auth.orgId)))
			.limit(1)

		if (!server) {
			set.status = 404
			return { error: "Server not found" }
		}
		return server
	}, { params: t.Object({ id: t.String() }) })
	.post(
		"/",
		async ({ headers, body, set }) => {
			const auth = await requireAuth(headers.authorization)
			assertRole(auth, "admin")

			const [server] = await db
				.insert(servers)
				.values({
					name: body.name,
					host: body.host,
					ip: body.ip,
					status: body.provision ? "provisioning" : "offline",
					orgId: auth.orgId,
				})
				.returning()

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "server.create",
				resourceType: "server",
				resourceId: server.id,
				meta: { name: body.name, host: body.host, provision: body.provision },
			})

			if (body.provision && body.domain) {
				provisionServer({
					serverId: server.id,
					host: body.host,
					domain: body.domain,
					orgId: auth.orgId,
				}).catch((e) => console.error("Provisioning error:", e))
			}

			return server
		},
		{
			body: t.Object({
				name: t.String(),
				host: t.String(),
				ip: t.Optional(t.String()),
				provision: t.Optional(t.Boolean()),
				domain: t.Optional(t.String()),
			}),
		},
	)
	.patch(
		"/:id",
		async ({ headers, params, body, set }) => {
			const auth = await requireAuth(headers.authorization)
			assertRole(auth, "admin")

			const updates: Record<string, unknown> = {}
			if (body.kubeconfig !== undefined) updates.kubeconfigEnc = body.kubeconfig
			if (body.status !== undefined) updates.status = body.status
			if (body.ip !== undefined) updates.ip = body.ip

			const [updated] = await db
				.update(servers)
				.set(updates)
				.where(and(eq(servers.id, Number(params.id)), eq(servers.orgId, auth.orgId)))
				.returning()

			if (!updated) {
				set.status = 404
				return { error: "Server not found" }
			}

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "server.update",
				resourceType: "server",
				resourceId: updated.id,
				meta: { fields: Object.keys(updates) },
			})

			return updated
		},
		{
			params: t.Object({ id: t.String() }),
			body: t.Object({
				kubeconfig: t.Optional(t.String()),
				status: t.Optional(t.String()),
				ip: t.Optional(t.String()),
			}),
		},
	)
	.delete("/:id", async ({ headers, params, set }) => {
		const auth = await requireAuth(headers.authorization)
		assertRole(auth, "admin")

		const [deleted] = await db
			.delete(servers)
			.where(and(eq(servers.id, Number(params.id)), eq(servers.orgId, auth.orgId)))
			.returning()

		if (!deleted) {
			set.status = 404
			return { error: "Server not found" }
		}

		await db.insert(auditLog).values({
			orgId: auth.orgId,
			userId: auth.userId,
			action: "server.delete",
			resourceType: "server",
			resourceId: deleted.id,
		})

		return { ok: true }
	}, { params: t.Object({ id: t.String() }) })
