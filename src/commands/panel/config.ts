import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { PanelClient } from "../../lib/panel"

async function configureCloudflare(panel: PanelClient) {
	const cfToken = await p.text({
		message: "Cloudflare API token",
		placeholder: "from dash.cloudflare.com/profile/api-tokens",
		validate: (v) => (!v ? "Required" : undefined),
	})
	if (p.isCancel(cfToken)) return

	const cfAccount = await p.text({
		message: "Cloudflare Account ID",
		placeholder: "from dashboard sidebar",
		validate: (v) => (!v ? "Required" : undefined),
	})
	if (p.isCancel(cfAccount)) return

	const spinner = p.spinner()
	spinner.start("Verifying token...")

	try {
		const result = await panel.req<{ cloudflareConfigured: boolean }>("/org/settings", {
			method: "PATCH",
			body: JSON.stringify({ cloudflareToken: cfToken, cloudflareAccountId: cfAccount }),
		})
		spinner.stop(result.cloudflareConfigured ? "Cloudflare configured ✓" : "Failed to verify token")
	} catch (e) {
		spinner.stop(`Failed: ${e instanceof Error ? e.message : e}`)
	}
}

async function configureGitHubApp(panel: PanelClient) {
	const org = await panel.req<{ githubOrg: string; githubAppConfigured: boolean }>("/org")

	if (org.githubAppConfigured) {
		const overwrite = await p.confirm({ message: "GitHub App already configured. Overwrite?", initialValue: false })
		if (p.isCancel(overwrite) || !overwrite) return
	}

	const panelUrl = panel.baseUrl
	const spinner = p.spinner()

	// GitHub App Manifest flow
	const result = await new Promise<{ appId: number; clientId: string; clientSecret: string } | null>((resolve) => {
		const timeout = setTimeout(() => { server.stop(); resolve(null) }, 120_000)

		const manifest = JSON.stringify({
			name: `Atlas (${org.githubOrg})`,
			url: panelUrl,
			hook_attributes: { url: `${panelUrl}/webhooks/github`, active: false },
			redirect_url: `REDIRECT_PLACEHOLDER`,
			callback_urls: [`${panelUrl}/auth/github/callback`],
			public: false,
			default_permissions: { members: "read", emails: "read" },
			default_events: [],
			request_oauth_on_install: true,
		})

		const server = Bun.serve({
			port: 0,
			async fetch(req): Promise<Response> {
				const url = new URL(req.url)

				// Serve the form page that auto-submits to GitHub
				if (url.pathname === "/") {
					const m: string = manifest.replace("REDIRECT_PLACEHOLDER", `http://localhost:${server.port}/callback`)
					const html: string = `<!DOCTYPE html>
<html><body>
<p>Redirecting to GitHub...</p>
<form id="f" action="https://github.com/organizations/${org.githubOrg}/settings/apps/new" method="post">
<input type="hidden" name="manifest" value='${m.replace(/'/g, "&#39;")}'>
</form>
<script>document.getElementById("f").submit()</script>
</body></html>`
					return new Response(html, { headers: { "Content-Type": "text/html" } })
				}

				// Receive callback from GitHub with code
				if (url.pathname === "/callback") {
					const code = url.searchParams.get("code")
					if (!code) return new Response("Missing code", { status: 400 })

					// Exchange code for app credentials
					const res = await fetch(`https://api.github.com/app-manifests/${code}/conversions`, {
						method: "POST",
						headers: { Accept: "application/vnd.github+json" },
					})
					if (!res.ok) {
						clearTimeout(timeout)
						setTimeout(() => server.stop(), 500)
						resolve(null)
						return new Response("<h1>❌ Failed to create GitHub App</h1>", { headers: { "Content-Type": "text/html" } })
					}

					const data = (await res.json()) as { id: number; client_id: string; client_secret: string }
					clearTimeout(timeout)
					setTimeout(() => server.stop(), 500)
					resolve({ appId: data.id, clientId: data.client_id, clientSecret: data.client_secret })

					return new Response("<h1>✅ GitHub App created! You can close this tab.</h1>", {
						headers: { "Content-Type": "text/html" },
					})
				}

				return new Response("Not found", { status: 404 })
			},
		})

		const setupUrl = `http://localhost:${server.port}/`
		spinner.stop(`Opening browser...`)
		Bun.spawn(["xdg-open", setupUrl], { stdout: "ignore", stderr: "ignore" })
		spinner.start("Waiting for GitHub App creation...")
	})

	if (!result) {
		spinner.stop("Timeout or failed")
		return
	}

	spinner.start("Saving credentials...")

	try {
		await panel.req("/org/settings", {
			method: "PATCH",
			body: JSON.stringify({
				githubAppId: result.appId,
				githubClientId: result.clientId,
				githubClientSecret: result.clientSecret,
			}),
		})
		spinner.stop(`GitHub App created ✓ (ID: ${result.appId}, Client: ${result.clientId})`)
	} catch (e) {
		spinner.stop(`Failed to save: ${e instanceof Error ? e.message : e}`)
		console.log(`\nManual save — Client ID: ${result.clientId}`)
	}
}

export default defineCommand({
	meta: { name: "config", description: "Configure organization settings" },
	args: {
		"cloudflare-token": { type: "string", description: "Cloudflare API token (non-interactive)" },
		"cloudflare-account": { type: "string", description: "Cloudflare account ID (non-interactive)" },
	},
	async run({ args }) {
		const panel = await PanelClient.create()
		if (!panel) {
			console.error("Not connected to Control Panel. Run: atlas panel setup")
			return
		}

		// Non-interactive mode
		if (args["cloudflare-token"]) {
			const body: Record<string, string> = { cloudflareToken: args["cloudflare-token"] }
			if (args["cloudflare-account"]) body.cloudflareAccountId = args["cloudflare-account"]
			const result = await panel.req<{ cloudflareConfigured: boolean }>("/org/settings", {
				method: "PATCH",
				body: JSON.stringify(body),
			})
			console.log(result.cloudflareConfigured ? "✓ Cloudflare configured" : "✗ Failed")
			return
		}

		p.intro(pc.bgMagenta(pc.black(" atlas panel config ")))

		const action = await p.select({
			message: "What do you want to configure?",
			options: [
				{ value: "github", label: "GitHub App (OAuth)", hint: "creates app automatically via browser" },
				{ value: "cloudflare", label: "Cloudflare (DNS)", hint: "API token for automatic DNS" },
			],
		})
		if (p.isCancel(action)) return

		if (action === "github") await configureGitHubApp(panel)
		if (action === "cloudflare") await configureCloudflare(panel)

		p.outro(pc.green("Done"))
	},
})
