import * as p from "@clack/prompts";
import { $ } from "bun";
import { defineCommand } from "citty";
import pc from "picocolors";
import { loadConfig, saveConfig } from "../lib/config";
import { openBrowser } from "../lib/browser";

async function ghInstalled(): Promise<boolean> {
	try {
		await $`gh --version`.quiet();
		return true;
	} catch {
		return false;
	}
}

async function ghToken(): Promise<string | null> {
	try {
		const r = await $`gh auth token`.quiet();
		const t = r.stdout.toString().trim();
		return t || null;
	} catch {
		return null;
	}
}

async function ghUser(): Promise<string | null> {
	try {
		const r = await $`gh api user --jq .login`.quiet();
		return r.stdout.toString().trim() || null;
	} catch {
		return null;
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
		const auto = args.yes;
		if (!auto) p.intro(pc.bgGreen(pc.black(" atlas login ")));

		const config = await loadConfig();

		// ── Panel OAuth flow ──
		if (config.panelUrl) {
			const log = auto
				? {
						start: (m: string) => console.log(`→ ${m}`),
						stop: (m: string) => console.log(`✓ ${m}`),
					}
				: p.spinner();

			log.start("Starting OAuth flow via Control Panel...");

			// Start local server to receive callback
			const result = await new Promise<{
				token: string;
				user: string;
				org: string;
				role: string;
			} | null>((resolve) => {
				const timeout = setTimeout(() => {
					server.stop();
					resolve(null);
				}, 120_000);
				const server = Bun.serve({
					port: 0,
					fetch(req) {
						const url = new URL(req.url);
						if (url.pathname === "/callback") {
							const token = url.searchParams.get("token");
							const user = url.searchParams.get("user");
							const org = url.searchParams.get("org");
							const role = url.searchParams.get("role");
							if (token && user && org && role) {
								clearTimeout(timeout);
								setTimeout(() => server.stop(), 500);
								resolve({ token, user, org, role });
								return new Response("<h1>✅ Authenticated! You can close this tab.</h1>", {
									headers: { "Content-Type": "text/html" },
								});
							}
						}
						return new Response("Waiting for auth...", { status: 400 });
					},
				});

				const authUrl = `${config.panelUrl}/auth/github?cli_port=${server.port}`;
				log.stop("Opening browser...");
				// Fire-and-forget browser open (inside Promise callback, can't await)
				openBrowser(authUrl).then((opened) => {
					if (!opened && !auto) p.log.warn(`Could not open browser. Visit:\n${authUrl}`);
				});
				if (!auto) p.log.info(`If the browser didn't open, visit:\n${authUrl}`);
				log.start("Waiting for GitHub authorization...");
			});

			if (!result) {
				log.stop("Timeout — no response from GitHub");
				return;
			}

			log.stop(`Authenticated as ${pc.cyan(result.user)} (${result.org}, ${result.role})`);

			await saveConfig({ panelToken: result.token });

			if (!auto) p.outro(pc.green("Ready to deploy!"));
			return;
		}

		// ── Legacy flow (gh CLI / manual token) ──
		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner();

		let token: string | null = args.token || process.env.GITHUB_TOKEN || null;
		let user: string | null = null;

		// Strategy 1: Use gh CLI (preferred)
		if (!token && (await ghInstalled())) {
			log.start("Checking gh auth...");

			// Check if already logged in with right scopes
			const existing = await ghToken();
			if (existing) {
				// Verify scopes include write:packages
				const scopeRes = await fetch("https://api.github.com/user", {
					headers: { Authorization: `Bearer ${existing}` },
				});
				const scopes = scopeRes.headers.get("x-oauth-scopes") || "";

				if (scopes.includes("write:packages")) {
					token = existing;
					user = await ghUser();
					log.stop(`Using gh auth (${pc.cyan(user || "unknown")}) — write:packages ✓`);
				} else {
					log.stop("gh token needs write:packages scope — launching gh auth");
					console.log();
					try {
						const proc = Bun.spawn(
							["gh", "auth", "refresh", "--hostname", "github.com", "--scopes", "write:packages"],
							{ stdin: "inherit", stdout: "inherit", stderr: "inherit" },
						);
						if ((await proc.exited) === 0) {
							token = await ghToken();
							user = await ghUser();
							console.log();
							log.start("");
							log.stop(`Scopes updated for ${pc.cyan(user || "unknown")}`);
						} else {
							console.log();
							log.start("");
							log.stop("gh auth refresh failed — falling back to manual token");
						}
					} catch {
						log.start("");
						log.stop("gh auth refresh failed — falling back to manual token");
					}
				}
			} else {
				log.stop("Not logged in to gh — launching gh auth login");
				if (!auto) {
					console.log();
					try {
						const proc = Bun.spawn(["gh", "auth", "login", "--scopes", "write:packages"], {
							stdin: "inherit",
							stdout: "inherit",
							stderr: "inherit",
						});
						if ((await proc.exited) === 0) {
							token = await ghToken();
							user = await ghUser();
						}
					} catch {}
					console.log();
					log.start("");
					log.stop(token ? `Authenticated as ${pc.cyan(user || "unknown")}` : "gh login failed");
				}
			}
		}

		// Strategy 2: Manual token
		if (!token && !auto) {
			for (let attempt = 0; attempt < 3; attempt++) {
				const input = await p.text({
					message: "GitHub PAT (needs write:packages + repo scope)",
					placeholder: "ghp_...",
					validate: (v) => (!v ? "Token is required" : undefined),
				});
				if (p.isCancel(input)) return p.cancel("Cancelled");

				log.start("Validating token...");
				const res = await fetch("https://api.github.com/user", {
					headers: { Authorization: `Bearer ${input}` },
				});
				if (res.ok) {
					token = input as string;
					user = ((await res.json()) as { login: string }).login;
					log.stop(`Authenticated as ${pc.cyan(user)}`);
					break;
				}
				log.stop("Invalid token — try again");
			}
		}

		if (!token && auto) {
			console.error("No token. Install gh CLI or pass --token / GITHUB_TOKEN");
			return;
		}
		if (!token) return;

		// Validate + get user if not from gh (for --token flag or GITHUB_TOKEN)
		if (!user) {
			log.start("Validating token...");
			const res = await fetch("https://api.github.com/user", {
				headers: { Authorization: `Bearer ${token}` },
			});
			if (!res.ok) {
				log.stop("Invalid token");
				return;
			}
			user = ((await res.json()) as { login: string }).login;
			log.stop(`Authenticated as ${pc.cyan(user)}`);
		}

		// GHCR login
		let dockerAvailable = true;
		try {
			await $`docker --version`.quiet();
		} catch {
			dockerAvailable = false;
		}

		if (dockerAvailable) {
			log.start("Logging into GHCR...");
			const ghcr = Bun.spawn(["docker", "login", "ghcr.io", "-u", user, "--password-stdin"], {
				stdin: new TextEncoder().encode(token),
				stdout: "pipe",
				stderr: "pipe",
			});
			if ((await ghcr.exited) !== 0) {
				log.stop("GHCR login failed (token may need write:packages scope)");
				console.error(await new Response(ghcr.stderr).text());
				return;
			}
			log.stop("GHCR authenticated");
		} else {
			if (!auto) p.log.warn("Docker not found — skipping GHCR login. Install Docker to enable deploys.");
			else console.log("⚠ Docker not found — skipping GHCR login");
		}

		// Save config
		await saveConfig({ githubToken: token, githubUser: user });

		// Update cluster pull secret if host configured
		const currentConfig = await loadConfig();
		if (currentConfig.host) {
			log.start("Updating cluster pull secret...");
			const { ssh } = await import("@atlas/ssh");
			const runtime = currentConfig.runtime || "k3s";
			const ns = currentConfig.domain ? currentConfig.domain.split(".")[0] : "default";

			let cmd: string;
			if (runtime === "swarm") {
				cmd = `echo "${token}" | docker login ghcr.io -u "${user}" --password-stdin`;
			} else if (runtime === "firecracker") {
				cmd = `mkdir -p /opt/atlas/firecracker && echo '{"auths":{"ghcr.io":{"auth":"'$(echo -n "${user}:${token}" | base64)'"}}}' > /opt/atlas/firecracker/registry-auth.json`;
			} else {
				cmd = `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl -n ${ns} create secret docker-registry ghcr-auth \
  --docker-server=ghcr.io \
  --docker-username="${user}" \
  --docker-password="${token}" \
  --dry-run=client -o yaml | kubectl apply -f -`;
			}

			const result = await ssh(currentConfig.host, cmd);
			if (result.ok) {
				log.stop("Cluster pull secret updated");
			} else {
				log.stop(`Cluster pull secret update failed (${runtime})`);
			}
		}

		// ── Cloudflare Auth (auto-detect or prompt) ──
		const existingCf = currentConfig.cloudflareToken;
		let cfToken = existingCf || null;
		let cfAccountId = currentConfig.cloudflareAccountId || null;

		if (!existingCf && !auto) {
			let cfTokenSource: "env" | "wrangler" | "manual" | null = null;

			// Strategy 1: env var
			if (process.env.CLOUDFLARE_API_TOKEN) {
				cfToken = process.env.CLOUDFLARE_API_TOKEN;
				cfTokenSource = "env";
				log.start("Cloudflare token found in CLOUDFLARE_API_TOKEN...");
			}

			// Strategy 2: wrangler CLI
			if (!cfToken) {
				try {
					const r = await $`wrangler auth token`.quiet();
					const lines = r.stdout.toString().trim().split("\n").filter(Boolean);
					const t = lines.at(-1)?.trim();
					if (t && !t.includes("wrangler") && !t.startsWith("─")) {
						cfToken = t;
						cfTokenSource = "wrangler";
						log.start("Cloudflare token found via wrangler...");
					}
				} catch {}
			}

			// Verify auto-detected token
			if (cfToken && !existingCf) {
				const { CloudflareClient } = await import("@atlas/cloudflare");
				const valid = await new CloudflareClient(cfToken, "").verify();
				if (valid) {
					log.stop("Cloudflare authenticated ✓");
					if (cfTokenSource === "wrangler") {
						p.log.warn(
							"Using wrangler OAuth token (may expire). For permanent access, create an API token\n" +
							`  at ${pc.cyan("dash.cloudflare.com/profile/api-tokens")} and re-run ${pc.cyan("atlas login")}.`,
						);
					}
				} else {
					log.stop("Auto-detected Cloudflare token is invalid");
					cfToken = null;
				}
			}

			// Strategy 3: manual prompt (fallback)
			if (!cfToken) {
				const wantCf = await p.confirm({
					message: "Configure Cloudflare for automatic DNS + HTTPS? (you can do this later)",
					initialValue: false,
				});
				if (!p.isCancel(wantCf) && wantCf) {
					p.log.step(
						[
							`Create a token at ${pc.cyan("dash.cloudflare.com/profile/api-tokens")}`,
							"  Permissions needed: Zone → DNS → Edit, Account → Cloudflare Tunnel → Edit",
						].join("\n"),
					);
					const cfInput = await p.text({
						message: "Cloudflare API token",
						placeholder: "paste your token here",
						validate: (v) => (!v ? "Token required" : undefined),
					});
					if (!p.isCancel(cfInput)) {
						cfToken = cfInput as string;

						log.start("Verifying Cloudflare token...");
						const { CloudflareClient } = await import("@atlas/cloudflare");
						const valid = await new CloudflareClient(cfToken, "").verify();
						if (!valid) {
							log.stop("Invalid Cloudflare token");
							cfToken = null;
						} else {
							log.stop("Cloudflare token valid");
						}
					}
				}
			}

			// Auto-detect account ID
			if (cfToken && !cfAccountId) {
				log.start("Detecting Cloudflare account...");
				try {
					const res = await fetch("https://api.cloudflare.com/client/v4/accounts?per_page=50", {
						headers: { Authorization: `Bearer ${cfToken}` },
					});
					const data = (await res.json()) as { result: { id: string; name: string }[] };
					const accounts = data.result ?? [];

					if (accounts.length === 1 && accounts[0]) {
						cfAccountId = accounts[0].id;
						log.stop(`Account: ${pc.cyan(accounts[0].name)} (${cfAccountId})`);
					} else if (accounts.length > 1) {
						log.stop(`Found ${accounts.length} Cloudflare accounts`);
						const selected = await p.select({
							message: "Which Cloudflare account?",
							options: accounts.map((a) => ({ value: a.id, label: `${a.name} (${a.id})` })),
						});
						if (!p.isCancel(selected)) cfAccountId = selected as string;
					} else {
						log.stop("No Cloudflare accounts found");
						const accInput = await p.text({
							message: "Cloudflare Account ID",
							placeholder: "found in dashboard sidebar",
						});
						if (!p.isCancel(accInput)) cfAccountId = accInput as string;
					}
				} catch {
					log.stop("Failed to detect account");
				}
			}
		}

		if (cfToken && cfAccountId) {
			await saveConfig({ cloudflareToken: cfToken, cloudflareAccountId: cfAccountId });
		}

		if (!auto) p.outro(pc.green("Ready to deploy!"));
		else console.log("✓ Ready to deploy!");
	},
});
