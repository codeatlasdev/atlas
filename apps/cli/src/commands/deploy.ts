import { defineCommand } from "citty"
import { $ } from "bun"
import { createCliRenderer, Box, Text } from "@opentui/core"
import { ssh } from "@atlas/ssh"
import { loadConfig } from "../lib/config"
import { loadProject } from "../lib/project"
import { createPanelClient, findProjectBySlug, waitForDeploy } from "../lib/panel"
import { theme, Header, StatusLine, Divider, MutableText, createSpinner } from "../ui"

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
		const sha = (await $`git rev-parse --short HEAD`.quiet()).stdout.toString().trim()
		const branch = (await $`git branch --show-current`.quiet()).stdout.toString().trim()
		const tag = args.tag || sha
		const registry = `ghcr.io/${project.org}/${project.name}`

		let services = Object.entries(project.services)
		if (args.service) {
			services = services.filter(([name]) => name === args.service)
			if (!services.length) { console.error(`Service "${args.service}" not in atlas.yaml`); return }
		}

		// ── Non-interactive mode ──
		if (auto) {
			const log = createSpinner(true)
			await runPipeline(services, registry, tag, project, config, args, log)
			console.log(`✓ ${branch}@${tag} is live!`)
			return
		}

		// ── Interactive TUI mode ──
		const renderer = await createCliRenderer()
		const root = Box({ width: "100%", flexDirection: "column", padding: 1, gap: 1 })

		root.add(Header("atlas deploy", `${branch}@${tag}`))
		root.add(Divider())

		const infoBox = Box({ flexDirection: "column" })
		infoBox.add(StatusLine("project ", project.name, theme.primary))
		infoBox.add(StatusLine("tag     ", tag, theme.warning))
		infoBox.add(StatusLine("services", services.map(([n]) => n).join(", "), theme.text))
		root.add(infoBox)
		root.add(Divider())

		// Step indicators
		const stepTexts: ReturnType<typeof MutableText>[] = []
		const allSteps = [
			...services.map(([n]) => `Build ${n}`),
			"Push to GHCR",
			"Deploy to cluster",
		]
		for (const name of allSteps) {
			const mt = MutableText(`  ○ ${name}`, theme.muted)
			stepTexts.push(mt)
			root.add(mt.node)
		}

		root.add(Text({ content: "", fg: theme.border }))
		const status = MutableText("  Ctrl+C to cancel", theme.muted)
		root.add(status.node)

		renderer.root.add(root)
		renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
			if (key.ctrl && key.name === "c") { renderer.destroy(); process.exit(0) }
		})

		const setStep = (i: number, state: "running" | "done" | "failed") => {
			const icon = state === "done" ? "✓" : state === "failed" ? "✗" : "◆"
			const color = state === "done" ? theme.success : state === "failed" ? theme.error : theme.primary
			stepTexts[i]!.update(`  ${icon} ${allSteps[i]}`, color)
			renderer.requestRender()
		}

		try {
			// Build
			for (let i = 0; i < services.length; i++) {
				const [name, svc] = services[i]!
				setStep(i, "running")
				const buildArgs = ["docker", "build", "-f", svc.dockerfile, "-t", `${registry}/${name}:${tag}`]
				if (svc.target) buildArgs.push("--target", svc.target)
				if (svc.buildArg) buildArgs.push("--build-arg", svc.buildArg)
				buildArgs.push(".")
				const build = Bun.spawn(buildArgs, { stdout: "pipe", stderr: "pipe" })
				if ((await build.exited) !== 0) { setStep(i, "failed"); await delay(2000); renderer.destroy(); return }
				setStep(i, "done")
			}

			// Push
			const pushIdx = services.length
			setStep(pushIdx, "running")
			for (const [name] of services) {
				const p = Bun.spawn(["docker", "push", `${registry}/${name}:${tag}`], { stdout: "pipe", stderr: "pipe" })
				if ((await p.exited) !== 0) { setStep(pushIdx, "failed"); await delay(2000); renderer.destroy(); return }
			}
			setStep(pushIdx, "done")

			// Deploy
			const deployIdx = pushIdx + 1
			setStep(deployIdx, "running")

			const panel = await createPanelClient()
			if (panel) {
				const proj = await findProjectBySlug(panel, project.name)
				if (proj) {
					const deploy = await panel.deploys.trigger({ projectId: proj.id, tag, services: args.service ? [args.service] : undefined })
					const result = await waitForDeploy(panel, deploy.id)
					setStep(deployIdx, result.status === "success" ? "done" : "failed")
				} else {
					setStep(deployIdx, "failed")
				}
			} else {
				const host = args.host || config.host
				if (host) {
					const ns = project.name
					const cmds = services.filter(([n]) => n !== "migrate").map(([n]) => `kubectl -n ${ns} set image deploy/${n} ${n}=${registry}/${n}:${tag} 2>/dev/null || true`).join("\n")
					await ssh(host, `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml\n${cmds}\nkubectl -n ${ns} rollout status deploy --timeout=120s 2>/dev/null || true`)
					setStep(deployIdx, "done")
				} else {
					setStep(deployIdx, "failed")
				}
			}

			status.update(`  ✓ ${branch}@${tag} is live!`, theme.success)
			renderer.requestRender()
			await delay(1500)
		} catch (e) {
			status.update(`  ✗ ${e}`, theme.error)
			renderer.requestRender()
			await delay(2000)
		}

		renderer.destroy()
	},
})

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function runPipeline(
	services: [string, any][],
	registry: string,
	tag: string,
	project: any,
	config: any,
	args: any,
	log: ReturnType<typeof createSpinner>,
) {
	for (const [name, svc] of services) {
		log.start(`Building ${name}:${tag}`)
		const buildArgs = ["docker", "build", "-f", svc.dockerfile, "-t", `${registry}/${name}:${tag}`]
		if (svc.target) buildArgs.push("--target", svc.target)
		if (svc.buildArg) buildArgs.push("--build-arg", svc.buildArg)
		buildArgs.push(".")
		const build = Bun.spawn(buildArgs, { stdout: "pipe", stderr: "pipe" })
		if ((await build.exited) !== 0) { log.fail(`${name} — FAILED`); return }
		log.stop(`${name}:${tag} built`)
	}

	log.start("Pushing to GHCR...")
	for (const [name] of services) {
		const p = Bun.spawn(["docker", "push", `${registry}/${name}:${tag}`], { stdout: "pipe", stderr: "pipe" })
		if ((await p.exited) !== 0) { log.fail("Push failed"); return }
	}
	log.stop("Images pushed")

	const panel = await createPanelClient()
	if (panel) {
		log.start("Deploying via Control Panel...")
		const proj = await findProjectBySlug(panel, project.name)
		if (!proj) { log.fail("Project not found"); return }
		const deploy = await panel.deploys.trigger({ projectId: proj.id, tag })
		const result = await waitForDeploy(panel, deploy.id)
		result.status === "success" ? log.stop(`Deployed ${tag} ✓`) : log.fail(`Deploy ${result.status}`)
	} else {
		const host = args.host || config.host
		if (!host) { log.fail("No host configured"); return }
		log.start("Deploying to cluster...")
		const ns = project.name
		const cmds = services.filter(([n]) => n !== "migrate").map(([n]) => `kubectl -n ${ns} set image deploy/${n} ${n}=${registry}/${n}:${tag} 2>/dev/null || true`).join("\n")
		await ssh(host, `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml\n${cmds}\nkubectl -n ${ns} rollout status deploy --timeout=120s 2>/dev/null || true`)
		log.stop(`Deployed ${tag}`)
	}
}
