import { getProvisionPhases } from "@atlas/provisioner";
import type { RuntimeType } from "@atlas/runtime";
import { ssh } from "@atlas/ssh";
import * as p from "@clack/prompts";
import { defineCommand } from "citty";
import pc from "picocolors";
import { loadConfig, saveConfig } from "../../lib/config";

const RUNTIME_TRADEOFFS: Record<RuntimeType, { pros: string[]; cons: string[] }> = {
	swarm: {
		pros: [
			"~100MB RAM overhead (vs ~512MB for K3s)",
			"Faster setup (~2min)",
			"Docker Compose native",
		],
		cons: ["No ArgoCD (GitOps)", "No auto-scaling (HPA)", "No Helm charts"],
	},
	k3s: {
		pros: ["Full Kubernetes ecosystem", "Helm charts, ArgoCD, HPA", "cert-manager for TLS"],
		cons: ["~512MB RAM overhead", "More complex", "Requires 2GB+ RAM"],
	},
	firecracker: {
		pros: [
			"~5MB per VM (hardware-level isolation)",
			"~125ms boot time",
			"No Docker in production",
		],
		cons: ["Requires bare metal with KVM", "Experimental", "No Helm/ArgoCD"],
	},
};

export default defineCommand({
	meta: {
		name: "setup",
		description: "Setup a fresh server with your chosen container runtime",
	},
	args: {
		host: { type: "string", description: "SSH host (e.g., root@1.2.3.4 or ssh alias)" },
		domain: { type: "string", description: "Base domain (e.g., myapp.com)" },
		runtime: { type: "string", description: "Container runtime: k3s, swarm, or firecracker" },
		"skip-monitoring": { type: "boolean", default: false },
		"skip-argocd": { type: "boolean", default: false },
		tunnel: { type: "boolean", default: false },
		"cf-token": { type: "string" },
		"cf-account": { type: "string" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes;
		if (!auto) p.intro(pc.bgCyan(pc.black(" atlas infra setup ")));

		const config = await loadConfig();

		// ── Host ──
		const host =
			args.host ||
			(auto
				? config.host
				: await p.text({
						message: "SSH host (e.g., root@1.2.3.4 or ssh config alias)",
						placeholder: config.host || "root@1.2.3.4",
						defaultValue: config.host,
						validate: (v) => (!v ? "Host is required" : undefined),
					}));
		if (!host || p.isCancel(host)) return auto ? undefined : p.cancel("Cancelled");

		// ── Domain ──
		const domain =
			args.domain ||
			(auto
				? config.domain
				: await p.text({
						message: "Base domain",
						placeholder: config.domain || "myapp.com",
						defaultValue: config.domain,
						validate: (v) => (!v ? "Domain is required" : undefined),
					}));
		if (!domain || p.isCancel(domain)) return auto ? undefined : p.cancel("Cancelled");

		// ── Runtime selection ──
		let runtime: RuntimeType;
		if (args.runtime === "k3s" || args.runtime === "swarm" || args.runtime === "firecracker") {
			runtime = args.runtime;
		} else if (auto) {
			runtime = (config.runtime as RuntimeType) || "k3s";
		} else {
			const selected = await p.select({
				message: "Container runtime",
				options: [
					{
						value: "swarm",
						label: `Docker Swarm ${pc.dim("(lightweight)")}`,
						hint: "~100MB overhead, docker-compose native, best for 1-2GB RAM",
					},
					{
						value: "k3s",
						label: `K3s / Kubernetes ${pc.dim("(full-featured)")}`,
						hint: "~512MB overhead, Helm, ArgoCD, HPA, best for 2GB+ RAM",
					},
					{
						value: "firecracker",
						label: `Firecracker ${pc.dim("(microVMs)")}`,
						hint: "~5MB per VM, hardware isolation, requires bare metal with KVM",
					},
				],
			});
			if (p.isCancel(selected)) return p.cancel("Cancelled");
			runtime = selected as RuntimeType;

			// Show tradeoffs
			const info = RUNTIME_TRADEOFFS[runtime];
			const runtimeLabel = runtime === "swarm" ? "Docker Swarm" : runtime === "k3s" ? "K3s" : "Firecracker";
			p.note(
				[
					...info.pros.map((t) => `  ${pc.green("✓")} ${t}`),
					...info.cons.map((t) => `  ${pc.yellow("✗")} ${t}`),
				].join("\n"),
				`${runtimeLabel} tradeoffs`,
			);
		}

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner();

		// ── SSH test ──
		log.start("Testing SSH connection...");
		try {
			const result = await ssh(host as string, "echo ok");
			if (!result.ok) throw new Error(result.stderr);
			log.stop("SSH connection OK");
		} catch (e) {
			log.stop("SSH connection failed");
			console.error(`Cannot connect to ${host}: ${e}`);
			return;
		}

		// ── Server info ──
		log.start("Checking server...");
		const info = await ssh(
			host as string,
			"echo $(grep PRETTY_NAME /etc/os-release | cut -d'\"' -f2) '|' $(free -h | awk '/Mem/{print $2}') RAM '|' $(nproc) vCPU",
		);
		log.stop(info.stdout.trim());

		// Detect server architecture for cross-platform builds
		const archResult = await ssh(host as string, "uname -m");
		const serverArch = archResult.stdout.trim();

		if (!auto) {
			const proceed = await p.confirm({
				message: `Setup ${pc.bold(host as string)} with ${pc.bold(runtime)} and domain ${pc.bold(domain as string)}?`,
			});
			if (p.isCancel(proceed) || !proceed) return p.cancel("Cancelled");
		}

		await saveConfig({ host: host as string, domain: domain as string, runtime, serverArch });

		// ── Tunnel ──
		const tunnel = args.tunnel
			? {
					cfToken: (args["cf-token"] || process.env.CLOUDFLARE_API_TOKEN) as string,
					cfAccount: (args["cf-account"] || process.env.CLOUDFLARE_ACCOUNT_ID) as string,
				}
			: undefined;

		if (args.tunnel && (!tunnel?.cfToken || !tunnel?.cfAccount)) {
			log.stop("Cloudflare Tunnel requires --cf-token and --cf-account");
			return;
		}

		// ── Provision ──
		const phases = getProvisionPhases({
			runtime,
			domain: domain as string,
			skipMonitoring: args["skip-monitoring"],
			skipArgocd: args["skip-argocd"],
			tunnel,
		});

		for (const phase of phases) {
			log.start(phase.name);
			const result = await ssh(host as string, phase.script);
			if (!result.ok) {
				log.stop(`${phase.name} — FAILED`);
				console.error(result.stderr || result.stdout);
				return;
			}
			log.stop(`${phase.name} — done`);
		}

		// ── Post-setup info ──
		const serverIp = await ssh(
			host as string,
			"curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'",
		);

		const notes = [
			`${pc.bold("DNS")} — Point to ${pc.cyan(serverIp.stdout.trim())}:`,
			`  *.${domain}`,
		];

		if (runtime === "k3s") {
			const creds = await ssh(
				host as string,
				"export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; echo \"ARGOCD_PASS=$(kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath='{.data.password}' 2>/dev/null | base64 -d 2>/dev/null || echo N/A)\"",
			);
			notes.push("", creds.stdout.trim());
		}

		if (runtime === "swarm") {
			notes.push("", `${pc.bold("Runtime")}: Docker Swarm`);
			const grafanaPass = await ssh(
				host as string,
				"cat /opt/atlas/monitoring/.grafana-pass 2>/dev/null || echo 'N/A'",
			);
			notes.push(
				`${pc.bold("Grafana")}: https://grafana.${domain} (admin/${grafanaPass.stdout.trim()})`,
			);
		}

		if (runtime === "firecracker") {
			notes.push("", `${pc.bold("Runtime")}: Firecracker microVMs`);
			notes.push(`${pc.bold("VMM socket")}: /var/run/atlas-vmm.sock`);
			notes.push(`${pc.bold("Base rootfs")}: /opt/atlas/firecracker/rootfs/base.ext4`);
		}

		p.note(notes.filter(Boolean).join("\n"), "Setup complete");

		if (!auto) p.outro(pc.green("Server ready! Push to main to deploy."));
		else console.log("✓ Server ready!");
	},
});
