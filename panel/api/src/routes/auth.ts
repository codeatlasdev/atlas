import { Elysia, t } from "elysia"
import { handleGitHubCallback, getGitHubAuthUrl } from "../lib/auth"
import { signToken } from "../lib/guard"

export const authRoutes = new Elysia({ prefix: "/auth" })
	.get("/github", async ({ query, set }) => {
		// Encode CLI port in state so callback knows where to redirect
		const cliPort = (query as Record<string, string>).cli_port
		const state = cliPort ? `cli:${cliPort}:${crypto.randomUUID()}` : crypto.randomUUID()
		set.redirect = await getGitHubAuthUrl(state)
	})
	.get(
		"/github/callback",
		async ({ query, set }) => {
			try {
				const { user, org } = await handleGitHubCallback(query.code)
				const token = await signToken({
					sub: String(user.id),
					org: String(org.id),
					role: user.role,
					username: user.githubUsername,
				})

				// If state starts with "cli:", redirect token to CLI's local server
				if (query.state?.startsWith("cli:")) {
					const port = query.state.split(":")[1]
					set.redirect = `http://localhost:${port}/callback?token=${token}&user=${user.githubUsername}&org=${org.slug}&role=${user.role}`
					return
				}

				return { token, user: { id: user.id, username: user.githubUsername, role: user.role, org: org.slug } }
			} catch (e) {
				set.status = 401
				return { error: e instanceof Error ? e.message : "Auth failed" }
			}
		},
		{ query: t.Object({ code: t.String(), state: t.Optional(t.String()) }) },
	)
	.post(
		"/token",
		async ({ body, set }) => {
			try {
				const { user, org } = await handleGitHubCallback(body.code)
				const token = await signToken({
					sub: String(user.id),
					org: String(org.id),
					role: user.role,
					username: user.githubUsername,
				})
				return { token, user: { id: user.id, username: user.githubUsername, role: user.role, org: org.slug } }
			} catch (e) {
				set.status = 401
				return { error: e instanceof Error ? e.message : "Auth failed" }
			}
		},
		{ body: t.Object({ code: t.String() }) },
	)
