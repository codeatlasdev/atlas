import { Elysia, t } from "elysia"
import { eq } from "drizzle-orm"
import { db } from "../db"
import { organizations, auditLog } from "../db/schema"
import { requireAuth, assertRole } from "../lib/guard"
import { CloudflareService } from "../services/cloudflare"
import { encrypt } from "../lib/crypto"

export const orgRoutes = new Elysia({ prefix: "/org" })
	.get("/", async ({ headers }) => {
		const auth = await requireAuth(headers.authorization)
		const org = await db.query.organizations.findFirst({
			where: eq(organizations.id, auth.orgId),
		})
		if (!org) return { error: "Org not found" }
		return {
			id: org.id,
			name: org.name,
			slug: org.slug,
			githubOrg: org.githubOrg,
			cloudflareConfigured: !!org.cloudflareTokenEnc,
			cloudflareAccountId: org.cloudflareAccountId,
			githubAppConfigured: !!org.githubClientId,
			githubAppId: org.githubAppId,
		}
	})
	.patch(
		"/settings",
		async ({ headers, body, set }) => {
			const auth = await requireAuth(headers.authorization)
			assertRole(auth, "admin")

			const updates: Record<string, unknown> = {}

			// Cloudflare
			if (body.cloudflareToken && body.cloudflareAccountId) {
				const cf = new CloudflareService(body.cloudflareToken, body.cloudflareAccountId)
				const valid = await cf.verify()
				if (!valid) {
					set.status = 400
					return { error: "Invalid Cloudflare token" }
				}
				updates.cloudflareTokenEnc = body.cloudflareToken
				updates.cloudflareAccountId = body.cloudflareAccountId
			}

			// GitHub PAT
			if (body.githubToken) {
				updates.githubTokenEnc = body.githubToken
			}

			// GitHub App (OAuth)
			if (body.githubAppId) updates.githubAppId = body.githubAppId
			if (body.githubClientId) updates.githubClientId = body.githubClientId
			if (body.githubClientSecret) {
				updates.githubClientSecretEnc = await encrypt(body.githubClientSecret)
			}

			const [updated] = await db
				.update(organizations)
				.set(updates)
				.where(eq(organizations.id, auth.orgId))
				.returning()

			await db.insert(auditLog).values({
				orgId: auth.orgId,
				userId: auth.userId,
				action: "org.settings.update",
				resourceType: "organization",
				resourceId: auth.orgId,
				meta: { fields: Object.keys(updates).map((k) => k.replace("Enc", "").replace("Token", "")) },
			})

			return {
				id: updated.id,
				name: updated.name,
				cloudflareConfigured: !!updated.cloudflareTokenEnc,
				cloudflareAccountId: updated.cloudflareAccountId,
				githubAppConfigured: !!updated.githubClientId,
				githubAppId: updated.githubAppId,
			}
		},
		{
			body: t.Object({
				cloudflareToken: t.Optional(t.String()),
				cloudflareAccountId: t.Optional(t.String()),
				githubToken: t.Optional(t.String()),
				githubAppId: t.Optional(t.Number()),
				githubClientId: t.Optional(t.String()),
				githubClientSecret: t.Optional(t.String()),
			}),
		},
	)
