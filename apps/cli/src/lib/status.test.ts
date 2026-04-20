import { describe, expect, test } from "bun:test";
import { getMockStatusData, parseStatusOutput } from "./status";

describe("parseStatusOutput", () => {
	test("parses K3s output correctly", () => {
		const stdout = `RUNTIME
K3s v1.31.4+k3s1

NODE
dev-matheus   v1.31.4+k3s1   4   8053Mi

APP_SERVICES
atlas-server-7d4f8b6c9-x2k4m Running 3d
atlas-worker-5c8d9e7f1-q3r5t Pending 1m

INFRA_SERVICES
kube-system 5/5
cert-manager 3/3
monitoring 6/7

MEMORY
2048/7892MB (26%)`;

		const data = parseStatusOutput(stdout, "k3s");

		expect(data.runtime).toBe("K3s v1.31.4+k3s1");
		expect(data.nodes).toEqual(["dev-matheus   v1.31.4+k3s1   4   8053Mi"]);
		expect(data.appServices).toEqual([
			{ name: "atlas-server-7d4f8b6c9-x2k4m", status: "Running", extra: "3d" },
			{ name: "atlas-worker-5c8d9e7f1-q3r5t", status: "Pending", extra: "1m" },
		]);
		expect(data.infraServices).toEqual([
			{ name: "kube-system", counts: "5/5", healthy: true },
			{ name: "cert-manager", counts: "3/3", healthy: true },
			{ name: "monitoring", counts: "6/7", healthy: false },
		]);
		expect(data.memory).toBe("2048/7892MB (26%)");
	});

	test("parses Swarm output correctly", () => {
		const stdout = `RUNTIME
Docker Swarm

NODE
demo-server   Ready   Active   Leader

APP_SERVICES
atlas_server 3/3 ghcr.io/acme/atlas/server:abc123

INFRA_SERVICES
traefik 1/1
monitoring 6/6

MEMORY
1024/7892MB (13%)`;

		const data = parseStatusOutput(stdout, "swarm");

		expect(data.runtime).toBe("Docker Swarm");
		expect(data.appServices).toHaveLength(1);
		expect(data.appServices[0]?.name).toBe("atlas_server");
		expect(data.infraServices).toHaveLength(2);
		expect(data.infraServices.every((s) => s.healthy)).toBe(true);
	});

	test("handles empty output", () => {
		const data = parseStatusOutput("", "k3s");
		expect(data.runtime).toBe("");
		expect(data.nodes).toEqual([]);
		expect(data.appServices).toEqual([]);
		expect(data.infraServices).toEqual([]);
		expect(data.memory).toBe("");
	});

	test("handles (no pods) marker", () => {
		const stdout = `RUNTIME
K3s v1.31.4

APP_SERVICES
(no pods)

MEMORY
512/2048MB (25%)`;

		const data = parseStatusOutput(stdout, "k3s");
		expect(data.appServices).toEqual([
			{ name: "(no", status: "pods)", extra: undefined },
		]);
	});
});

describe("getMockStatusData", () => {
	test("returns K3s mock data", () => {
		const data = getMockStatusData("k3s");
		expect(data.runtime).toContain("K3s");
		expect(data.appServices.length).toBeGreaterThan(0);
		expect(data.infraServices.length).toBeGreaterThan(0);
		expect(data.memory).toBeTruthy();
	});

	test("returns Swarm mock data", () => {
		const data = getMockStatusData("swarm");
		expect(data.runtime).toBe("Docker Swarm");
		expect(data.appServices.length).toBeGreaterThan(0);
		expect(data.infraServices.some((s) => s.name === "traefik")).toBe(true);
	});

	test("defaults to K3s for unknown runtime", () => {
		const data = getMockStatusData("firecracker");
		expect(data.runtime).toContain("K3s");
	});
});
