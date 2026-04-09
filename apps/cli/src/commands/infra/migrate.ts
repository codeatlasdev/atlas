import { getProvisionPhases } from "@atlas/provisioner";
import type { RuntimeType } from "@atlas/runtime";
import { ssh } from "@atlas/ssh";
import * as p from "@clack/prompts";
import { defineCommand } from "citty";
import pc from "picocolors";
import { loadConfig, saveConfig } from "../../lib/config";

export default defineCommand({
	meta: {
		name: "migrate",
		description: "Migrate server between container runtimes (K3s ↔ Swarm)",
	},
	args: {
		to: { type: "string", description: "Target runtime: k3s or swarm", required: true },
		host: { type: "string", description: "SSH host" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes;
		const config = await loadConfig();
		const host = args.host || config.host;
		const domain = config.domain;
		const currentRuntime = config.runtime || "k3s";

		if (!host) {
			console.error("No host configured. Run: atlas infra setup");
			return;
		}
		if (!domain) {
			console.error("No domain configured. Run: atlas infra setup");
			return;
		}

		const target = args.to as RuntimeType;
		if (target !== "k3s" && target !== "swarm") {
			console.error("Invalid runtime. Use: --to k3s or --to swarm");
			return;
		}
		if (target === currentRuntime) {
			console.log(`Server is already running ${currentRuntime}. Nothing to do.`);
			return;
		}

		if (!auto) {
			p.intro(pc.bgYellow(pc.black(" atlas infra migrate ")));

			p.note(
				[
					`${pc.bold("Current runtime")}: ${currentRuntime}`,
					`${pc.bold("Target runtime")}:  ${target}`,
					"",
					pc.yellow("⚠ Risks:"),
					"  • Services will experience downtime during migration",
					"  • Persistent volumes are NOT migrated automatically",
					"  • All services will be re-deployed on the new runtime",
					"  • Secrets will be re-synced automatically",
					"",
					target === "swarm"
						? [
								pc.yellow("⚠ You will lose:"),
								"  • ArgoCD (GitOps)",
								"  • HPA (auto-scaling)",
								"  • Helm chart support",
								"  • cert-manager (Traefik ACME replaces it)",
							].join("\n")
						: [
								pc.green("✓ You will gain:"),
								"  • ArgoCD (GitOps)",
								"  • HPA (auto-scaling)",
								"  • Helm chart support",
								"  • cert-manager",
							].join("\n"),
				].join("\n"),
				"Migration plan",
			);

			const proceed = await p.confirm({
				message: `Migrate ${pc.bold(host)} from ${pc.bold(currentRuntime)} to ${pc.bold(target)}?`,
			});
			if (p.isCancel(proceed) || !proceed) return p.cancel("Cancelled");
		}

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner();

		// Step 1: List current services
		log.start("Discovering running services...");
		let serviceList: string[] = [];
		const ns = domain.split(".")[0];

		if (currentRuntime === "k3s") {
			const svcResult = await ssh(
				host,
				`export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl -n ${ns} get deploy --no-headers -o custom-columns=':metadata.name' 2>/dev/null`,
			);
			serviceList = svcResult.stdout.trim().split("\n").filter(Boolean);
		} else {
			const svcResult = await ssh(
				host,
				`docker service ls --filter "label=com.docker.stack.namespace=${ns}" --format "{{.Name}}" 2>/dev/null`,
			);
			serviceList = svcResult.stdout
				.trim()
				.split("\n")
				.filter(Boolean)
				.map((s) => s.replace(`${ns}_`, ""));
		}

		if (serviceList.length > 0) {
			log.stop(`Found ${serviceList.length} service(s): ${serviceList.join(", ")}`);
		} else {
			log.stop("No services found (clean migration)");
		}

		// Step 2: Install new runtime
		log.start(`Installing ${target} runtime...`);
		const phases = getProvisionPhases({
			runtime: target,
			domain,
			skipMonitoring: false,
		});

		for (const phase of phases) {
			log.start(phase.name);
			const result = await ssh(host, phase.script);
			if (!result.ok) {
				log.stop(`${phase.name} — FAILED`);
				console.error(result.stderr || result.stdout);
				return;
			}
			log.stop(`${phase.name} — done`);
		}

		// Step 3: Cleanup old runtime
		log.start(`Cleaning up ${currentRuntime}...`);
		if (currentRuntime === "k3s") {
			await ssh(host, "/usr/local/bin/k3s-uninstall.sh 2>/dev/null || true");
		} else {
			await ssh(host, "docker swarm leave --force 2>/dev/null || true");
		}
		log.stop(`${currentRuntime} removed`);

		// Step 4: Update config
		await saveConfig({ runtime: target });

		if (!auto) {
			p.note(
				[
					`Runtime migrated to ${pc.bold(target)}`,
					"",
					serviceList.length > 0
						? `${pc.yellow("⚠ Action required:")} Re-deploy your services:\n  ${pc.dim("atlas deploy")}`
						: "No services to re-deploy.",
					"",
					"Verify:",
					"  • DNS is pointing correctly",
					"  • atlas status",
				].join("\n"),
				"Migration complete",
			);
			p.outro(pc.green("Migration successful!"));
		} else {
			console.log(`✓ Migrated to ${target}`);
			if (serviceList.length > 0) {
				console.log("→ Re-deploy needed: atlas deploy");
			}
		}
	},
});
