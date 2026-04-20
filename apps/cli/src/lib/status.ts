/**
 * Status data layer — parsing, types, and mock data for `atlas status`.
 *
 * Separated from the command to enable:
 * - `--demo` mode (mock data, no SSH)
 * - Snapshot testing of the TUI
 * - Future `--json` output
 */

// ── Types ──

export interface StatusData {
	runtime: string;
	nodes: string[];
	appServices: { name: string; status: string; extra?: string }[];
	infraServices: { name: string; counts: string; healthy: boolean }[];
	memory: string;
}

// ── Parser ──

export function parseStatusOutput(stdout: string, runtime: string): StatusData {
	const data: StatusData = {
		runtime: "",
		nodes: [],
		appServices: [],
		infraServices: [],
		memory: "",
	};

	let section = "";
	for (const line of stdout.split("\n")) {
		const trimmed = line.trim();
		if (["RUNTIME", "NODE", "APP_SERVICES", "INFRA_SERVICES", "MEMORY"].includes(trimmed)) {
			section = trimmed;
			continue;
		}
		if (!trimmed) continue;

		switch (section) {
			case "RUNTIME":
				data.runtime = trimmed;
				break;
			case "NODE":
				data.nodes.push(trimmed);
				break;
			case "APP_SERVICES": {
				const parts = trimmed.split(/\s+/);
				data.appServices.push({
					name: parts[0] ?? "",
					status: parts[1] ?? "",
					extra: parts.slice(2).join(" ") || undefined,
				});
				break;
			}
			case "INFRA_SERVICES": {
				const [name, counts] = trimmed.split(/\s+/);
				const [running, total] = (counts ?? "").split("/");
				data.infraServices.push({
					name: name ?? "",
					counts: counts ?? "",
					healthy: running === total,
				});
				break;
			}
			case "MEMORY":
				data.memory = trimmed;
				break;
		}
	}

	return data;
}

// ── Mock data ──

export function getMockStatusData(runtime: string): StatusData {
	if (runtime === "swarm") {
		return {
			runtime: "Docker Swarm",
			nodes: ["demo-server   Ready   Active   Leader"],
			appServices: [
				{ name: "atlas_server", status: "3/3", extra: "ghcr.io/acme/atlas/server:a1b2c3d" },
				{ name: "atlas_worker", status: "1/1", extra: "ghcr.io/acme/atlas/worker:a1b2c3d" },
				{ name: "atlas_web", status: "2/2", extra: "ghcr.io/acme/atlas/web:a1b2c3d" },
			],
			infraServices: [
				{ name: "traefik", counts: "1/1", healthy: true },
				{ name: "monitoring", counts: "6/6", healthy: true },
			],
			memory: "1024/7892MB (13%)",
		};
	}

	return {
		runtime: "K3s v1.31.4+k3s1",
		nodes: ["demo-server   v1.31.4+k3s1   4   8053Mi"],
		appServices: [
			{ name: "atlas-server-7d4f8b6c9-x2k4m", status: "Running", extra: "3d" },
			{ name: "atlas-server-7d4f8b6c9-p8n2j", status: "Running", extra: "3d" },
			{ name: "atlas-worker-5c8d9e7f1-q3r5t", status: "Running", extra: "1d" },
			{ name: "atlas-web-8e2a4c6d3-m7n9p", status: "Running", extra: "2d" },
		],
		infraServices: [
			{ name: "kube-system", counts: "5/5", healthy: true },
			{ name: "cert-manager", counts: "3/3", healthy: true },
			{ name: "monitoring", counts: "7/7", healthy: true },
			{ name: "argocd", counts: "4/4", healthy: true },
		],
		memory: "2148/7892MB (27%)",
	};
}
