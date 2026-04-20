import type { ProvisionPhase, RuntimeType } from "@atlas/api/types";
import { getK3sPhases, phase01System } from "./k3s";
import { getSwarmPhases } from "./swarm";

export type { ProvisionPhase } from "@atlas/api/types";

export interface ProvisionOptions {
	runtime: RuntimeType;
	domain: string;
	skipMonitoring?: boolean;
	skipArgocd?: boolean;
	tunnel?: { cfToken: string; cfAccount: string };
}

export function getProvisionPhases(opts: ProvisionOptions): ProvisionPhase[] {
	if (opts.runtime === "swarm") return getSwarmPhases(opts);
	if (opts.runtime === "firecracker") {
		const { getFirecrackerPhases } = require("@atlas/firecracker");
		return getFirecrackerPhases(opts) as ProvisionPhase[];
	}
	return getK3sPhases(opts);
}

export interface JoinOptions {
	runtime: RuntimeType;
	managerHost: string;
	managerIp: string;
	token: string;
}

export function getJoinPhases(opts: JoinOptions): ProvisionPhase[] {
	const phases: ProvisionPhase[] = [{ name: "System preparation", script: phase01System() }];

	if (opts.runtime === "swarm") {
		phases.push({
			name: "Join Swarm cluster",
			script: `set -euo pipefail
if ! command -v docker &> /dev/null; then
  curl -fsSL https://get.docker.com | sh > /dev/null 2>&1
  systemctl enable docker
  systemctl start docker
fi
docker swarm join --token ${opts.token} ${opts.managerIp}:2377
echo "ok"`,
		});
	} else {
		phases.push({
			name: "Join K3s cluster",
			script: `set -euo pipefail
if ! command -v k3s &> /dev/null; then
  curl -sfL https://get.k3s.io | K3S_URL=https://${opts.managerIp}:6443 K3S_TOKEN=${opts.token} sh -
  sleep 10
fi
echo "ok"`,
		});
	}

	return phases;
}
