import type { RuntimeService, RuntimeType } from "@atlas/api/types";
import { K3sRuntime } from "./k3s";
import { SwarmRuntime } from "./swarm";

export type { RuntimeType, RuntimeService } from "@atlas/api/types";

export { K3sRuntime } from "./k3s";
export { SwarmRuntime } from "./swarm";

export function createRuntime(type: RuntimeType, host: string): RuntimeService {
	if (type === "swarm") return new SwarmRuntime(host);
	if (type === "firecracker") {
		// Lazy import to avoid hard dependency — firecracker package is optional
		const { FirecrackerRuntime } = require("@atlas/firecracker");
		return new FirecrackerRuntime(host) as RuntimeService;
	}
	return new K3sRuntime(host);
}
