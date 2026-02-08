import { defineCommand } from "citty"
import { $ } from "bun"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadConfig } from "../lib/config"
import { loadProject } from "../lib/project"
import { ssh } from "../lib/ssh"
import { PanelClient } from "../lib/panel"

export default defineCommand({
	meta: { name: "deploy", description: "Build, push, and deploy to the cluster" },
	args: {
		tag: { type: "string", description: "Image tag (default: git short SHA)" },
		service: { type: "string", alias: "s", description: "Deploy only this service" },
		yes: { type: "boolean", alias: "y", default: false },
		host: { type: "string", description: "SSH host (legacy mode)" },
	},
	async run({ args }) {
		const config = await loadConfig()
		const project = await loadProject()

		if (!project) { console.error("No atlas.yaml found. Run: atlas create"); return }

		const auto = args.yes
		if (!auto) p.intro(pc.bgMagenta(pc.black(" atlas deploy ")))

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner()

		const sha = (await $`git rev-parse --short HEAD`.quiet()).stdout.toString().trim()
		const branch = (await $`git branch --show-current`.quiet()).stdout.toString().trim()
		const tag = args.tag || sha
		const registry = `ghcr.io/${project.org}/${project.name}`

		// Filter services
		let services = Object.entries(project.services)
		if (args.service) {
			services = services.filter(([name]) => name === args.service)
			if (!services.length) { console.error(`Service "${args.service}" not in atlas.yaml`); return }
		}

		if (!auto) {
			const proceed = await p.confirm({
				message: `Deploy ${pc.bold(branch)}@${pc.cyan(tag)}? (${services.length} services)`,
			})
			if (p.isCancel(proceed) || !proceed) return p.cancel("Cancelled")
		}

		// ── Build ──
		for (const [name, svc] of services) {
			log.start(`Building ${name}:${tag}`)
			const buildArgs = ["docker", "build", "-f", svc.dockerfile, "-t", `${registry}/${name}:${tag}`]
			if (svc.target) buildArgs.push("--target", svc.target)
			if (svc.buildArg) buildArgs.push("--build-arg", svc.buildArg)
			buildArgs.push(".")

			const build = Bun.spawn(buildArgs, { stdout: "pipe", stderr: "pipe" })
			if ((await build.exited) !== 0) {
				log.stop(`${name} — FAILED`)
				console.error(await new Response(build.stderr).text())
				return
			}
			log.stop(`${name}:${tag} built`)
		}

		// ── Push ──
		log.start("Pushing to GHCR...")
		for (const [name] of services) {
			const push = Bun.spawn(["docker", "push", `${registry}/${name}:${tag}`], { stdout: "pipe", stderr: "pipe" })
			if ((await push.exited) !== 0) {
				log.stop("Push failed")
				console.error(await new Response(push.stderr).text())
				return
			}
		}
		log.stop("Images pushed")

		// ── Deploy via Control Panel or SSH ──
		const panel = await PanelClient.create()

		if (panel) {
			await deployViaPanel(panel, project, tag, services, log, args.service)
		} else {
			const host = args.host || config.host
			if (!host) { console.error("No host and no panel configured. Run: atlas infra setup"); return }
			await deployViaSSH(host, project, registry, tag, services, config, log)
		}

		if (!auto) p.outro(pc.green(`${branch}@${tag} is live!`))
		else console.log(`✓ ${branch}@${tag} is live!`)
	},
})

// ── Deploy via Control Panel API ──

async function deployViaPanel(
	panel: PanelClient,
	project: { name: string; org: string },
	tag: string,
	services: [string, unknown][],
	log: { start: (m: string) => void; stop: (m: string) => void },
	serviceFilter?: string,
) {
	log.start("Deploying via Control Panel...")

	const proj = await panel.findProjectBySlug(project.name)
	if (!proj) {
		log.stop("Project not found in Control Panel. Register it first.")
		return
	}

	const serviceNames = serviceFilter ? [serviceFilter] : undefined
	const deploy = await panel.triggerDeploy(proj.id, tag, serviceNames)
	log.stop(`Deploy triggered (id: ${deploy.id})`)

	log.start("Waiting for deploy...")
	const result = await panel.waitForDeploy(deploy.id)

	if (result.status === "success") {
		log.stop(`Deployed ${tag} ✓`)
	} else {
		log.stop(`Deploy ${result.status}`)
	}
}

// ── Legacy: Deploy via SSH ──

async function deployViaSSH(
	host: string,
	project: { name: string; org: string; domain?: string; services: Record<string, { domain?: string }> },
	registry: string,
	tag: string,
	services: [string, { domain?: string }][],
	config: { cloudflareToken?: string; cloudflareAccountId?: string; tunnelId?: string; domain?: string },
	log: { start: (m: string) => void; stop: (m: string) => void },
) {
	log.start("Deploying to cluster...")
	const ns = project.name
	const deployCommands = services
		.filter(([name]) => name !== "migrate")
		.map(([name]) => `kubectl -n ${ns} set image deploy/${name} ${name}=${registry}/${name}:${tag} 2>/dev/null || true`)
		.join("\n")

	const migrateCmd = services.some(([n]) => n === "migrate")
		? `kubectl -n ${ns} delete job migrate 2>/dev/null || true
kubectl -n ${ns} apply -f - <<'EOF'
apiVersion: batch/v1
kind: Job
metadata:
  name: migrate
  namespace: ${ns}
spec:
  backoffLimit: 3
  ttlSecondsAfterFinished: 300
  template:
    spec:
      restartPolicy: OnFailure
      imagePullSecrets:
        - name: ghcr-auth
      containers:
        - name: migrate
          image: ${registry}/migrate:${tag}
          envFrom:
            - secretRef:
                name: ${ns}-secrets
EOF`
		: ""

	await ssh(host, `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
${migrateCmd}
${deployCommands}
kubectl -n ${ns} rollout status deploy --timeout=120s 2>/dev/null || true
echo "deployed"`)

	log.stop(`Deployed ${tag}`)

	// Auto DNS
	if (config.cloudflareToken && config.cloudflareAccountId && config.tunnelId) {
		const domainsToSetup = services
			.filter(([, svc]) => svc.domain)
			.map(([name, svc]) => ({ name, domain: svc.domain! }))

		if (domainsToSetup.length > 0) {
			log.start("Configuring DNS...")
			try {
				const { CloudflareClient } = await import("../lib/cloudflare")
				const cf = new CloudflareClient(config.cloudflareToken, config.cloudflareAccountId)
				const baseDomain = project.domain || config.domain
				if (baseDomain) {
					const zone = await cf.getZoneByName(baseDomain)
					if (zone) {
						const results: string[] = []
						for (const { domain } of domainsToSetup) {
							const status = await cf.ensureTunnelDNS(zone.id, domain, config.tunnelId)
							results.push(`${domain} → ${status}`)
						}
						log.stop(`DNS: ${results.join(", ")}`)
					}
				}
			} catch (e) {
				log.stop(`DNS setup failed: ${e}`)
			}
		}
	}
}
