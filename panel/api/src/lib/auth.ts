import { eq } from "drizzle-orm"
import { db } from "../db"
import { organizations, users } from "../db/schema"
import { decrypt } from "./crypto"

interface GitHubUser {
	id: number
	login: string
	avatar_url: string
	email: string | null
}

interface GitHubOrg {
	login: string
}

async function getOAuthCredentials(orgId?: number): Promise<{ clientId: string; clientSecret: string }> {
	// Try DB first
	if (orgId) {
		const org = await db.query.organizations.findFirst({ where: eq(organizations.id, orgId) })
		if (org?.githubClientId && org?.githubClientSecretEnc) {
			return {
				clientId: org.githubClientId,
				clientSecret: await decrypt(org.githubClientSecretEnc),
			}
		}
	}
	// Try any org with credentials
	const orgs = await db.select().from(organizations)
	for (const org of orgs) {
		if (org.githubClientId && org.githubClientSecretEnc) {
			return {
				clientId: org.githubClientId,
				clientSecret: await decrypt(org.githubClientSecretEnc),
			}
		}
	}
	// Fallback to env
	return {
		clientId: process.env.GITHUB_CLIENT_ID ?? "",
		clientSecret: process.env.GITHUB_CLIENT_SECRET ?? "",
	}
}

export async function getGitHubAuthUrl(state: string): Promise<string> {
	const { clientId } = await getOAuthCredentials()
	const params = new URLSearchParams({
		client_id: clientId,
		scope: "read:org user:email",
		state,
	})
	return `https://github.com/login/oauth/authorize?${params}`
}

async function exchangeCode(code: string): Promise<string> {
	const { clientId, clientSecret } = await getOAuthCredentials()
	const res = await fetch("https://github.com/login/oauth/access_token", {
		method: "POST",
		headers: { Accept: "application/json", "Content-Type": "application/json" },
		body: JSON.stringify({ client_id: clientId, client_secret: clientSecret, code }),
	})
	const data = (await res.json()) as { access_token?: string; error?: string }
	if (!data.access_token) throw new Error(data.error ?? "GitHub OAuth failed")
	return data.access_token
}

async function fetchGitHubUser(token: string): Promise<GitHubUser> {
	const res = await fetch("https://api.github.com/user", {
		headers: { Authorization: `Bearer ${token}`, Accept: "application/vnd.github+json" },
	})
	if (!res.ok) throw new Error("Failed to fetch GitHub user")
	return res.json() as Promise<GitHubUser>
}

async function fetchGitHubOrgs(token: string): Promise<GitHubOrg[]> {
	const res = await fetch("https://api.github.com/user/orgs", {
		headers: { Authorization: `Bearer ${token}`, Accept: "application/vnd.github+json" },
	})
	if (!res.ok) throw new Error("Failed to fetch GitHub orgs")
	return res.json() as Promise<GitHubOrg[]>
}

export async function handleGitHubCallback(code: string) {
	const accessToken = await exchangeCode(code)
	const [ghUser, ghOrgs] = await Promise.all([
		fetchGitHubUser(accessToken),
		fetchGitHubOrgs(accessToken),
	])

	// Find matching org
	const orgs = await db.select().from(organizations)
	const matchedOrg = orgs.find((org) =>
		ghOrgs.some((ghOrg) => ghOrg.login.toLowerCase() === org.githubOrg.toLowerCase()),
	)

	if (!matchedOrg) {
		throw new Error(`User ${ghUser.login} is not a member of any registered organization`)
	}

	// Upsert user
	const existing = await db.select().from(users).where(eq(users.githubId, ghUser.id)).limit(1)

	let user: typeof users.$inferSelect

	if (existing.length > 0) {
		const [updated] = await db
			.update(users)
			.set({
				githubUsername: ghUser.login,
				avatarUrl: ghUser.avatar_url,
				email: ghUser.email,
			})
			.where(eq(users.githubId, ghUser.id))
			.returning()
		user = updated
	} else {
		// First user in org becomes admin
		const orgUsers = await db.select().from(users).where(eq(users.orgId, matchedOrg.id)).limit(1)
		const role = orgUsers.length === 0 ? "admin" as const : "dev" as const

		const [created] = await db
			.insert(users)
			.values({
				githubId: ghUser.id,
				githubUsername: ghUser.login,
				avatarUrl: ghUser.avatar_url,
				email: ghUser.email,
				role,
				orgId: matchedOrg.id,
			})
			.returning()
		user = created
	}

	return { user, org: matchedOrg }
}
