import { defineCommand } from "citty"
import { $ } from "bun"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadConfig, saveConfig } from "../lib/config"

async function ghInstalled(): Promise<boolean> {
	try {
		await $`gh --version`.quiet()
		return true
	} catch {
		return false
	}
}

async function ghToken(): Promise<string | null> {
	try {
		const r = await $`gh auth token`.quiet()
		const t = r.stdout.toString().trim()
		return t || null
	} catch {
		return null
	}
}

async function ghUser(): Promise<string | null> {
	try {
		const r = await $`gh api user --jq .login`.quiet()
		return r.stdout.toString().trim() || null
	} catch {
		return null
	}
}

export default defineCommand({
	meta: {
		name: "login",
		description: "Authenticate with GitHub (GHCR + API access)",
	},
	args: {
		token: {
			type: "string",
			description: "GitHub PAT (skip gh auth flow)",
		},
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes
		if (!auto) p.intro(pc.bgGreen(pc.black(" atlas login ")))

		const config = await loadConfig()

		// ── Panel OAuth flow ──
		if (config.panelUrl) {
			const log = auto
				? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
				: p.spinner()

			log.start("Starting OAuth flow via Control Panel...")

			// Start local server to receive callback
			const result = await new Promise<{ token: string; user: string; org: string; role: string } | null>((resolve) => {
				const timeout = setTimeout(() => { server.stop(); resolve(null) }, 120_000)
				const server = Bun.serve({
					port: 0,
					fetch(req) {
						const url = new URL(req.url)
						if (url.pathname === "/callback") {
							const token = url.searchParams.get("token")
							const user = url.searchParams.get("user")
							const org = url.searchParams.get("org")
							const role = url.searchParams.get("role")
							if (token && user && org && role) {
								clearTimeout(timeout)
								setTimeout(() => server.stop(), 500)
								resolve({ token, user, org, role })
								return new Response("<h1>✅ Authenticated! You can close this tab.</h1>", {
									headers: { "Content-Type": "text/html" },
								})
							}
						}
						return new Response("Waiting for auth...", { status: 400 })
					},
				})

				const authUrl = `${config.panelUrl}/auth/github?cli_port=${server.port}`
				log.stop(`Opening browser...`)
				Bun.spawn(["xdg-open", authUrl], { stdout: "ignore", stderr: "ignore" })
				log.start("Waiting for GitHub authorization...")
			})

			if (!result) {
				log.stop("Timeout — no response from GitHub")
				return
			}

			log.stop(`Authenticated as ${pc.cyan(result.user)} (${result.org}, ${result.role})`)

			await saveConfig({ panelToken: result.token })

			if (!auto) p.outro(pc.green("Ready to deploy!"))
			return
		}

		// ── Legacy flow (gh CLI / manual token) ──
		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner()

		let token: string | null = args.token || process.env.GITHUB_TOKEN || null
		let user: string | null = null

		// Strategy 1: Use gh CLI (preferred)
		if (!token && (await ghInstalled())) {
			log.start("Checking gh auth...")

			// Check if already logged in with right scopes
			const existing = await ghToken()
			if (existing) {
				// Verify scopes include write:packages
				const scopeRes = await fetch("https://api.github.com/user", {
					headers: { Authorization: `Bearer ${existing}` },
				})
				const scopes = scopeRes.headers.get("x-oauth-scopes") || ""

				if (scopes.includes("write:packages")) {
					token = existing
					user = await ghUser()
					log.stop(`Using gh auth (${pc.cyan(user || "unknown")}) — write:packages ✓`)
				} else {
					log.stop("gh token missing write:packages scope")
					log.start("Re-authenticating with write:packages...")
					try {
						// Interactive: opens browser for OAuth device flow
						const proc = Bun.spawn(
							["gh", "auth", "refresh", "--hostname", "github.com", "--scopes", "write:packages"],
							{ stdin: "inherit", stdout: "inherit", stderr: "inherit" },
						)
						if ((await proc.exited) === 0) {
							token = await ghToken()
							user = await ghUser()
							log.stop(`Scopes updated for ${pc.cyan(user || "unknown")}`)
						} else {
							log.stop("gh auth refresh failed — falling back to manual token")
						}
					} catch {
						log.stop("gh auth refresh failed — falling back to manual token")
					}
				}
			} else {
				log.stop("Not logged in to gh")
				if (!auto) {
					log.start("Starting gh auth login...")
					try {
						// Interactive gh login with write:packages
						const proc = Bun.spawn(
							["gh", "auth", "login", "--scopes", "write:packages"],
							{ stdin: "inherit", stdout: "inherit", stderr: "inherit" },
						)
						if ((await proc.exited) === 0) {
							token = await ghToken()
							user = await ghUser()
						}
					} catch {}
					log.stop(token ? `Authenticated as ${pc.cyan(user || "unknown")}` : "gh login failed")
				}
			}
		}

		// Strategy 2: Manual token
		if (!token && !auto) {
			const input = await p.text({
				message: "GitHub PAT (needs write:packages + repo scope)",
				placeholder: "ghp_...",
				validate: (v) => (!v ? "Token is required" : undefined),
			})
			if (p.isCancel(input)) return p.cancel("Cancelled")
			token = input as string
		}

		if (!token) {
			console.error("No token. Install gh CLI or pass --token")
			return
		}

		// Validate + get user if not from gh
		if (!user) {
			log.start("Validating token...")
			const res = await fetch("https://api.github.com/user", {
				headers: { Authorization: `Bearer ${token}` },
			})
			if (!res.ok) {
				log.stop("Invalid token")
				return
			}
			user = ((await res.json()) as { login: string }).login
			log.stop(`Authenticated as ${pc.cyan(user)}`)
		}

		// GHCR login
		log.start("Logging into GHCR...")
		const ghcr = Bun.spawn(
			["docker", "login", "ghcr.io", "-u", user, "--password-stdin"],
			{ stdin: new TextEncoder().encode(token), stdout: "pipe", stderr: "pipe" },
		)
		if ((await ghcr.exited) !== 0) {
			log.stop("GHCR login failed (token may need write:packages scope)")
			console.error(await new Response(ghcr.stderr).text())
			return
		}
		log.stop("GHCR authenticated")

		// Save config
		await saveConfig({ githubToken: token, githubUser: user })

		// Update cluster pull secret if host configured
		const currentConfig = await loadConfig()
		if (currentConfig.host) {
			log.start("Updating cluster pull secret...")
			const { ssh } = await import("../lib/ssh")
			const ns = currentConfig.domain ? currentConfig.domain.split(".")[0] : "default"
			await ssh(currentConfig.host, `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} create secret docker-registry ghcr-auth \
  --docker-server=ghcr.io \
  --docker-username="${user}" \
  --docker-password="${token}" \
  --dry-run=client -o yaml | kubectl apply -f -`)
			log.stop("Cluster pull secret updated")
		}

		// ── Cloudflare Auth (optional) ──
		const existingCf = currentConfig.cloudflareToken
		let cfToken = existingCf || null
		let cfAccountId = currentConfig.cloudflareAccountId || null

		if (!existingCf && !auto) {
			const wantCf = await p.confirm({
				message: "Configure Cloudflare? (tunnels + DNS automation)",
				initialValue: true,
			})
			if (!p.isCancel(wantCf) && wantCf) {
				const cfInput = await p.text({
					message: "Cloudflare API token (needs Tunnel:Edit + DNS:Edit)",
					placeholder: "paste token from dash.cloudflare.com/profile/api-tokens",
					validate: (v) => (!v ? "Token required" : undefined),
				})
				if (!p.isCancel(cfInput)) {
					cfToken = cfInput as string

					log.start("Verifying Cloudflare token...")
					const { CloudflareClient } = await import("../lib/cloudflare")
					const tmpClient = new CloudflareClient(cfToken, "")
					const valid = await tmpClient.verify()
					if (!valid) {
						log.stop("Invalid Cloudflare token")
						cfToken = null
					} else {
						log.stop("Cloudflare token valid")

						// Detect account ID from zones
						if (currentConfig.domain) {
							log.start(`Detecting account for ${currentConfig.domain}...`)
							try {
								const res = await fetch(`https://api.cloudflare.com/client/v4/zones?name=${currentConfig.domain}&per_page=1`, {
									headers: { Authorization: `Bearer ${cfToken}` },
								})
								const data = (await res.json()) as { result: { account: { id: string } }[] }
								if (data.result?.[0]?.account?.id) {
									cfAccountId = data.result[0].account.id
									log.stop(`Account: ${pc.cyan(cfAccountId)}`)
								} else {
									log.stop("Could not detect account ID")
									const accInput = await p.text({
										message: "Cloudflare Account ID",
										placeholder: "found in dashboard sidebar",
									})
									if (!p.isCancel(accInput)) cfAccountId = accInput as string
								}
							} catch {
								log.stop("Failed to detect account")
							}
						} else {
							const accInput = await p.text({
								message: "Cloudflare Account ID",
								placeholder: "found in dashboard sidebar",
							})
							if (!p.isCancel(accInput)) cfAccountId = accInput as string
						}
					}
				}
			}
		}

		if (cfToken && cfAccountId) {
			await saveConfig({ cloudflareToken: cfToken, cloudflareAccountId: cfAccountId })
		}

		if (!auto) p.outro(pc.green("Ready to deploy!"))
		else console.log("✓ Ready to deploy!")
	},
})
