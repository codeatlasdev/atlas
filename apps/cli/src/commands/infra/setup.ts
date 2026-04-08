import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { ssh } from "@atlas/ssh"
import { getProvisionPhases } from "@atlas/provisioner"
import { loadConfig, saveConfig } from "../../lib/config"

export default defineCommand({
	meta: {
		name: "setup",
		description: "Setup a fresh server with K3s, Traefik, cert-manager, monitoring, and ArgoCD",
	},
	args: {
		host: { type: "string", description: "SSH host (e.g., root@1.2.3.4 or ssh alias)" },
		domain: { type: "string", description: "Base domain (e.g., myapp.com)" },
		"skip-monitoring": { type: "boolean", default: false },
		"skip-argocd": { type: "boolean", default: false },
		tunnel: { type: "boolean", default: false },
		"cf-token": { type: "string" },
		"cf-account": { type: "string" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes
		if (!auto) p.intro(pc.bgCyan(pc.black(" atlas infra setup ")))

		const config = await loadConfig()

		const host =
			args.host ||
			(auto
				? config.host
				: await p.text({
						message: "SSH host (e.g., root@1.2.3.4 or ssh config alias)",
						placeholder: config.host || "root@1.2.3.4",
						defaultValue: config.host,
						validate: (v) => (!v ? "Host is required" : undefined),
					}))

		if (!host || p.isCancel(host)) return auto ? undefined : p.cancel("Cancelled")

		const domain =
			args.domain ||
			(auto
				? config.domain
				: await p.text({
						message: "Base domain",
						placeholder: config.domain || "myapp.com",
						defaultValue: config.domain,
						validate: (v) => (!v ? "Domain is required" : undefined),
					}))

		if (!domain || p.isCancel(domain)) return auto ? undefined : p.cancel("Cancelled")

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner()

		log.start("Testing SSH connection...")
		try {
			const result = await ssh(host as string, "echo ok")
			if (!result.ok) throw new Error(result.stderr)
			log.stop("SSH connection OK")
		} catch (e) {
			log.stop("SSH connection failed")
			console.error(`Cannot connect to ${host}: ${e}`)
			return
		}

		log.start("Checking server...")
		const info = await ssh(
			host as string,
			"echo $(grep PRETTY_NAME /etc/os-release | cut -d'\"' -f2) '|' $(free -h | awk '/Mem/{print $2}') RAM '|' $(nproc) vCPU",
		)
		log.stop(info.stdout.trim())

		if (!auto) {
			const proceed = await p.confirm({
				message: `Setup ${pc.bold(host as string)} with domain ${pc.bold(domain as string)}?`,
			})
			if (p.isCancel(proceed) || !proceed) return p.cancel("Cancelled")
		}

		await saveConfig({ host: host as string, domain: domain as string })

		const tunnel = args.tunnel
			? {
					cfToken: (args["cf-token"] || process.env.CLOUDFLARE_API_TOKEN) as string,
					cfAccount: (args["cf-account"] || process.env.CLOUDFLARE_ACCOUNT_ID) as string,
				}
			: undefined

		if (args.tunnel && (!tunnel?.cfToken || !tunnel?.cfAccount)) {
			log.stop("Cloudflare Tunnel requires --cf-token and --cf-account")
			return
		}

		const phases = getProvisionPhases({
			domain: domain as string,
			skipMonitoring: args["skip-monitoring"],
			skipArgocd: args["skip-argocd"],
			tunnel,
		})

		for (const phase of phases) {
			log.start(phase.name)
			const result = await ssh(host as string, phase.script)
			if (!result.ok) {
				log.stop(`${phase.name} — FAILED`)
				console.error(result.stderr || result.stdout)
				return
			}
			log.stop(`${phase.name} — done`)
		}

		const creds = await ssh(
			host as string,
			'export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; echo "ARGOCD_PASS=$(kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath=\'{.data.password}\' 2>/dev/null | base64 -d 2>/dev/null || echo N/A)"',
		)
		const serverIp = await ssh(
			host as string,
			"curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'",
		)

		p.note(
			[
				`${pc.bold("DNS")} — Point to ${pc.cyan(serverIp.stdout.trim())}:`,
				`  *.${domain}`,
				"",
				creds.stdout.trim(),
			]
				.filter(Boolean)
				.join("\n"),
			"Setup complete",
		)

		if (!auto) p.outro(pc.green("Server ready! Push to main to deploy."))
		else console.log("✓ Server ready!")
	},
})
