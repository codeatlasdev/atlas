import { ssh } from "@atlas/ssh";
import { Box, Text, createCliRenderer } from "@opentui/core";
import { $ } from "bun";
import { defineCommand } from "citty";
import { type AtlasConfig, loadConfig } from "../lib/config";
import { createPanelClient, findProjectBySlug, waitForDeploy } from "../lib/panel";
import { type ProjectConfig, type ServiceConfig, loadProject } from "../lib/project";
import { Divider, Header, MutableText, StatusLine, createSpinner, theme } from "../ui";

export default defineCommand({
	meta: { name: "deploy", description: "Build, push, and deploy to the cluster" },
	args: {
		tag: { type: "string", description: "Image tag (default: git short SHA)" },
		service: { type: "string", alias: "s", description: "Deploy only this service" },
		yes: { type: "boolean", alias: "y", default: false },
		host: { type: "string", description: "SSH host (legacy mode)" },
		"dry-run": {
			type: "boolean",
			description: "Simulate deploy TUI without executing (for dev/testing)",
			default: false,
		},
	},
	async run({ args }) {
		const config = await loadConfig();
		const project = await loadProject();

		// Resolve target platform for docker build
		const platform = resolvePlatform(project?.platform, config.serverArch);

		// ── Dry-run mode: simulate TUI without executing ──
		if (args["dry-run"]) {
			const serviceNames = project
				? Object.keys(project.services)
				: ["server", "worker", "web"];
			const name = project?.name ?? "atlas-demo";
			const tag = args.tag || "a1b2c3d";
			const branch = "main";
			await simulateDeploy(serviceNames, name, tag, branch);
			return;
		}

		if (!project) {
			console.error("No atlas.yaml found. Run: atlas create");
			return;
		}

		const auto = args.yes;
		const sha = (await $`git rev-parse --short HEAD`.quiet()).stdout.toString().trim();
		const branch = (await $`git branch --show-current`.quiet()).stdout.toString().trim();
		const tag = args.tag || sha;
		const registry = `ghcr.io/${project.org}/${project.name}`;

		let services = Object.entries(project.services);
		if (args.service) {
			services = services.filter(([name]) => name === args.service);
			if (!services.length) {
				console.error(`Service "${args.service}" not in atlas.yaml`);
				return;
			}
		}

		// ── Non-interactive mode ──
		if (auto) {
			const log = createSpinner(true);
			await runPipeline(services, registry, tag, project, config, args, log, platform);
			console.log(`✓ ${branch}@${tag} is live!`);
			return;
		}

		// ── Interactive TUI mode ──
		const renderer = await createCliRenderer();
		const root = Box({ width: "100%", flexDirection: "column", padding: 1, gap: 1 });

		root.add(Header("atlas deploy", `${branch}@${tag}`));
		root.add(Divider());

		const infoBox = Box({ flexDirection: "column" });
		infoBox.add(StatusLine("project ", project.name, theme.primary));
		infoBox.add(StatusLine("tag     ", tag, theme.warning));
		infoBox.add(StatusLine("services", services.map(([n]) => n).join(", "), theme.text));
		root.add(infoBox);
		root.add(Divider());

		// Step indicators
		const stepTexts: ReturnType<typeof MutableText>[] = [];
		const allSteps = [...services.map(([n]) => `Build ${n}`), "Push to GHCR", "Deploy to cluster"];
		for (const name of allSteps) {
			const mt = MutableText(`  ○ ${name}`, theme.muted);
			stepTexts.push(mt);
			root.add(mt.node);
		}

		root.add(Text({ content: "", fg: theme.border }));
		const status = MutableText("  Ctrl+C to cancel", theme.muted);
		root.add(status.node);

		renderer.root.add(root);
		renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
			if (key.ctrl && key.name === "c") {
				renderer.destroy();
				process.exit(0);
			}
		});

		const setStep = (i: number, state: "running" | "done" | "failed") => {
			const icon = state === "done" ? "✓" : state === "failed" ? "✗" : "◆";
			const color =
				state === "done" ? theme.success : state === "failed" ? theme.error : theme.primary;
			stepTexts[i]?.update(`  ${icon} ${allSteps[i]}`, color);
			renderer.requestRender();
		};

		try {
			// Build
			let alreadyPushed = false;
			for (let i = 0; i < services.length; i++) {
				const entry = services[i];
				if (!entry) continue;
				const [name, svc] = entry;
				setStep(i, "running");
				const { proc: build, pushed } = buildImage(
					svc.dockerfile,
					`${registry}/${name}:${tag}`,
					platform,
					{ target: svc.target, buildArg: svc.buildArg },
				);
				if ((await build.exited) !== 0) {
					setStep(i, "failed");
					await delay(2000);
					renderer.destroy();
					return;
				}
				if (pushed) alreadyPushed = true;
				setStep(i, "done");
			}

			// Push (skip if buildx already pushed)
			const pushIdx = services.length;
			if (alreadyPushed) {
				setStep(pushIdx, "done");
			} else {
				setStep(pushIdx, "running");
				for (const [name] of services) {
					const p = Bun.spawn(["docker", "push", `${registry}/${name}:${tag}`], {
						stdout: "pipe",
						stderr: "pipe",
					});
					if ((await p.exited) !== 0) {
						setStep(pushIdx, "failed");
						await delay(2000);
						renderer.destroy();
						return;
					}
				}
				setStep(pushIdx, "done");
			}

			// Deploy
			const deployIdx = pushIdx + 1;
			setStep(deployIdx, "running");

			const panel = await createPanelClient();
			if (panel) {
				const proj = await findProjectBySlug(panel, project.name);
				if (proj) {
					const deploy = await panel.deploys.trigger({
						projectId: proj.id,
						tag,
						services: args.service ? [args.service] : undefined,
					});
					const result = await waitForDeploy(panel, deploy.id);
					setStep(deployIdx, result.status === "success" ? "done" : "failed");
				} else {
					setStep(deployIdx, "failed");
				}
			} else {
				const host = args.host || config.host;
				if (host) {
					const { createRuntime } = await import("@atlas/runtime");
					const runtime = createRuntime(config.runtime || "k3s", host);
					const ns = project.name;

					await ensurePullSecret(host, ns, config);

					let allOk = true;
					for (const [n] of services.filter(([n]) => n !== "migrate")) {
						const ok = await runtime.deploy(ns, n, `${registry}/${n}:${tag}`);
						if (!ok) allOk = false;
					}

					if (allOk) {
						for (const [n] of services.filter(([n]) => n !== "migrate")) {
							await runtime.rolloutStatus(ns, n);
						}
						setStep(deployIdx, "done");
					} else {
						setStep(deployIdx, "failed");
					}
				} else {
					setStep(deployIdx, "failed");
				}
			}

			status.update(`  ✓ ${branch}@${tag} is live!`, theme.success);
			renderer.requestRender();
			await delay(1500);
		} catch (e) {
			status.update(`  ✗ ${e}`, theme.error);
			renderer.requestRender();
			await delay(2000);
		}

		renderer.destroy();
	},
});

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Create or update the GHCR pull secret in the target namespace using fresh credentials. */
async function ensurePullSecret(host: string, ns: string, config: AtlasConfig) {
	const user = config.githubUser;
	const token = config.githubToken;
	if (!user || !token) return;

	const runtime = config.runtime || "k3s";
	if (runtime === "k3s") {
		await ssh(host, `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create ns ${ns} 2>/dev/null || true
kubectl -n ${ns} create secret docker-registry ghcr-auth \
  --docker-server=ghcr.io \
  --docker-username="${user}" \
  --docker-password="${token}" \
  --dry-run=client -o yaml | kubectl apply -f -`);
	} else if (runtime === "swarm") {
		await ssh(host, `echo "${token}" | docker login ghcr.io -u "${user}" --password-stdin`);
	}
}

async function runPipeline(
	services: [string, ServiceConfig][],
	registry: string,
	tag: string,
	project: ProjectConfig,
	config: AtlasConfig,
	args: { host?: string; service?: string },
	log: ReturnType<typeof createSpinner>,
	platform: string,
) {
	let alreadyPushed = false;
	for (const [name, svc] of services) {
		log.start(`Building ${name}:${tag}`);
		const { proc: build, pushed } = buildImage(
			svc.dockerfile,
			`${registry}/${name}:${tag}`,
			platform,
			{ target: svc.target, buildArg: svc.buildArg },
		);
		if ((await build.exited) !== 0) {
			log.fail(`${name} — FAILED`);
			return;
		}
		if (pushed) alreadyPushed = true;
		log.stop(`${name}:${tag} built`);
	}

	if (alreadyPushed) {
		log.start("Images pushed (via buildx)");
		log.stop("Images pushed");
	} else {
		log.start("Pushing to GHCR...");
		for (const [name] of services) {
			const p = Bun.spawn(["docker", "push", `${registry}/${name}:${tag}`], {
				stdout: "pipe",
				stderr: "pipe",
			});
			if ((await p.exited) !== 0) {
				log.fail("Push failed");
				return;
			}
		}
		log.stop("Images pushed");
	}

	const panel = await createPanelClient();
	if (panel) {
		log.start("Deploying via Control Panel...");
		const proj = await findProjectBySlug(panel, project.name);
		if (!proj) {
			log.fail("Project not found");
			return;
		}
		const deploy = await panel.deploys.trigger({ projectId: proj.id, tag });
		const result = await waitForDeploy(panel, deploy.id);
		result.status === "success"
			? log.stop(`Deployed ${tag} ✓`)
			: log.fail(`Deploy ${result.status}`);
	} else {
		const host = args.host || config.host;
		if (!host) {
			log.fail("No host configured");
			return;
		}
		log.start("Deploying to cluster...");
		const { createRuntime } = await import("@atlas/runtime");
		const runtime = createRuntime(config.runtime || "k3s", host);
		const ns = project.name;

		await ensurePullSecret(host, ns, config);

		let allOk = true;
		for (const [n] of services.filter(([n]) => n !== "migrate")) {
			const ok = await runtime.deploy(ns, n, `${registry}/${n}:${tag}`);
			if (!ok) { allOk = false; break; }
		}

		if (allOk) {
			for (const [n] of services.filter(([n]) => n !== "migrate")) {
				await runtime.rolloutStatus(ns, n);
			}
			log.stop(`Deployed ${tag}`);
		} else {
			log.fail("Deploy failed");
		}
	}
}

async function simulateDeploy(
	serviceNames: string[],
	projectName: string,
	tag: string,
	branch: string,
) {
	const renderer = await createCliRenderer();
	const root = Box({ width: "100%", flexDirection: "column", padding: 1, gap: 1 });

	root.add(Header("atlas deploy", `${branch}@${tag} (dry-run)`));
	root.add(Divider());

	const infoBox = Box({ flexDirection: "column" });
	infoBox.add(StatusLine("project ", projectName, theme.primary));
	infoBox.add(StatusLine("tag     ", tag, theme.warning));
	infoBox.add(StatusLine("services", serviceNames.join(", "), theme.text));
	root.add(infoBox);
	root.add(Divider());

	const allSteps = [
		...serviceNames.map((n) => `Build ${n}`),
		"Push to GHCR",
		"Deploy to cluster",
	];
	const stepTexts: ReturnType<typeof MutableText>[] = [];
	for (const name of allSteps) {
		const mt = MutableText(`  ○ ${name}`, theme.muted);
		stepTexts.push(mt);
		root.add(mt.node);
	}

	root.add(Text({ content: "", fg: theme.border }));
	const status = MutableText("  Simulating deploy...", theme.muted);
	root.add(status.node);

	renderer.root.add(root);
	renderer.keyInput.on("keypress", (key: { ctrl: boolean; name: string }) => {
		if (key.ctrl && key.name === "c") {
			renderer.destroy();
			process.exit(0);
		}
	});

	const setStep = (i: number, state: "running" | "done" | "failed") => {
		const icon = state === "done" ? "✓" : state === "failed" ? "✗" : "◆";
		const color =
			state === "done" ? theme.success : state === "failed" ? theme.error : theme.primary;
		stepTexts[i]?.update(`  ${icon} ${allSteps[i]}`, color);
		renderer.requestRender();
	};

	for (let i = 0; i < allSteps.length; i++) {
		setStep(i, "running");
		await delay(400);
		setStep(i, "done");
	}

	status.update(`  ✓ dry-run complete — ${branch}@${tag}`, theme.success);
	renderer.requestRender();
	await delay(1500);
	renderer.destroy();
}

function resolvePlatform(projectPlatform?: string, serverArch?: string): string {
	// 1. Explicit override in atlas.yaml
	if (projectPlatform) return projectPlatform;
	// 2. Detected from server during infra setup
	if (serverArch) {
		const map: Record<string, string> = {
			x86_64: "linux/amd64",
			amd64: "linux/amd64",
			aarch64: "linux/arm64",
			arm64: "linux/arm64",
		};
		return map[serverArch] ?? "linux/amd64";
	}
	// 3. Default — most VPS are x86_64
	return "linux/amd64";
}

/** Build a Docker image for the target platform. Uses buildx for cross-compilation. */
function buildImage(
	dockerfile: string,
	imageTag: string,
	platform: string,
	opts?: { target?: string; buildArg?: string },
): { proc: ReturnType<typeof Bun.spawn>; pushed: boolean } {
	const needsCross = !platform.includes(process.arch === "arm64" ? "arm64" : "amd64");
	const args: string[] = needsCross
		? ["docker", "buildx", "build", "--platform", platform, "--push"]
		: ["docker", "build", "--platform", platform];

	args.push("-f", dockerfile, "-t", imageTag);
	if (opts?.target) args.push("--target", opts.target);
	if (opts?.buildArg) args.push("--build-arg", opts.buildArg);
	args.push(".");

	return {
		proc: Bun.spawn(args, { stdout: "pipe", stderr: "pipe" }),
		pushed: needsCross, // buildx --push already pushed
	};
}
