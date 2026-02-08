import { db } from "./db"
import { organizations, users } from "./db/schema"
import { signToken } from "./lib/guard"

const ORG_NAME = process.env.SEED_ORG_NAME ?? "MyOrg"
const ORG_SLUG = process.env.SEED_ORG_SLUG ?? ORG_NAME.toLowerCase().replace(/\s+/g, "-")
const GITHUB_ORG = process.env.SEED_GITHUB_ORG ?? ORG_SLUG
const ADMIN_USERNAME = process.env.SEED_ADMIN_USERNAME ?? "admin"
const ADMIN_EMAIL = process.env.SEED_ADMIN_EMAIL ?? ""

const [org] = await db
	.insert(organizations)
	.values({
		name: ORG_NAME,
		slug: ORG_SLUG,
		githubOrg: GITHUB_ORG,
	})
	.onConflictDoNothing()
	.returning()

if (org) {
	console.log(`✅ Organization created: ${org.name} (id: ${org.id})`)

	const [admin] = await db
		.insert(users)
		.values({
			githubId: 1,
			githubUsername: ADMIN_USERNAME,
			email: ADMIN_EMAIL || null,
			role: "admin",
			orgId: org.id,
		})
		.onConflictDoNothing()
		.returning()

	if (admin) {
		console.log(`✅ Admin user created: ${admin.githubUsername} (id: ${admin.id})`)

		const token = await signToken({
			sub: String(admin.id),
			org: String(org.id),
			role: admin.role,
			username: admin.githubUsername,
		})

		console.log(`\n🔑 Dev token (use in Authorization header):\nBearer ${token}\n`)
	}
} else {
	console.log("ℹ Organization already exists")
}

process.exit(0)
