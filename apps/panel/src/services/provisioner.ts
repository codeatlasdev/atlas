import { db } from "@atlas/db";
import { auditLog, servers } from "@atlas/db/schema";
import { getProvisionPhases } from "@atlas/provisioner";
import type { RuntimeType } from "@atlas/runtime";
import { ssh } from "@atlas/ssh";
import { eq } from "drizzle-orm";

interface ProvisionOptions {
	serverId: number;
	host: string;
	domain: string;
	runtime: RuntimeType;
	orgId: number;
	skipMonitoring?: boolean;
	skipArgocd?: boolean;
}

export async function provisionServer(opts: ProvisionOptions): Promise<void> {
	const { serverId, host, domain, runtime, orgId } = opts;
	const log = (msg: string) => console.log(`[provision:${serverId}] ${msg}`);

	try {
		log("Testing SSH...");
		const test = await ssh(host, "echo ok");
		if (!test.ok) throw new Error(`SSH failed: ${test.stderr}`);

		const info = await ssh(
			host,
			"echo $(nproc) vCPU / $(free -h | awk '/Mem/{print $2}') RAM / $(df -h / | awk 'NR==2{print $4}') free",
		);
		log(info.stdout.trim());

		const ipResult = await ssh(
			host,
			"curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'",
		);
		const ip = ipResult.stdout.trim();

		await db
			.update(servers)
			.set({ ip, status: "provisioning", runtime })
			.where(eq(servers.id, serverId));

		const phases = getProvisionPhases({
			runtime,
			domain,
			skipMonitoring: opts.skipMonitoring,
			skipArgocd: opts.skipArgocd,
		});

		for (const phase of phases) {
			log(`${phase.name}...`);
			const result = await ssh(host, phase.script);
			if (!result.ok) throw new Error(`${phase.name} failed: ${result.stderr || result.stdout}`);
			log(`${phase.name} ✓`);
		}

		// Store runtime-specific credentials
		let kubeconfigEnc: string | undefined;
		const meta: Record<string, unknown> = {
			provisionedAt: new Date().toISOString(),
			info: info.stdout.trim(),
			runtime,
		};

		if (runtime === "k3s") {
			const kcResult = await ssh(host, "cat /etc/rancher/k3s/k3s.yaml");
			const kubeconfig = kcResult.stdout.replace(/127\.0\.0\.1/g, ip);
			const { encrypt } = await import("@atlas/crypto");
			kubeconfigEnc = await encrypt(kubeconfig);
		} else {
			// Store Swarm join tokens for multi-node (encrypted)
			const { encrypt } = await import("@atlas/crypto");
			const tokenResult = await ssh(host, "docker swarm join-token worker -q");
			const managerTokenResult = await ssh(host, "docker swarm join-token manager -q");
			meta.swarmWorkerTokenEnc = await encrypt(tokenResult.stdout.trim());
			meta.swarmManagerTokenEnc = await encrypt(managerTokenResult.stdout.trim());
		}

		await db
			.update(servers)
			.set({
				status: "online",
				ip,
				...(kubeconfigEnc ? { kubeconfigEnc } : {}),
				meta,
			})
			.where(eq(servers.id, serverId));

		await db.insert(auditLog).values({
			orgId,
			action: "server.provisioned",
			resourceType: "server",
			resourceId: serverId,
			meta: { ip, runtime },
		});

		log("Server online ✓");
	} catch (e) {
		const error = e instanceof Error ? e.message : String(e);
		log(`FAILED: ${error}`);
		await db
			.update(servers)
			.set({ status: "error", meta: { error } })
			.where(eq(servers.id, serverId));
	}
}

export async function joinServerToCluster(opts: {
	serverId: number;
	host: string;
	runtime: RuntimeType;
	managerServerId: number;
	orgId: number;
}): Promise<void> {
	const { serverId, host, runtime, managerServerId, orgId } = opts;
	const log = (msg: string) => console.log(`[join:${serverId}] ${msg}`);

	try {
		const manager = await db.query.servers.findFirst({
			where: eq(servers.id, managerServerId),
		});
		if (!manager?.ip) throw new Error("Manager server has no IP");

		const managerMeta = manager.meta as Record<string, unknown> | null;
		let token: string;

		if (runtime === "swarm") {
			const enc = managerMeta?.swarmWorkerTokenEnc as string | undefined;
			if (enc) {
				const { decrypt } = await import("@atlas/crypto");
				token = await decrypt(enc);
			} else {
				// Fallback: fetch from manager directly
				const result = await ssh(manager.host, "docker swarm join-token worker -q");
				if (!result.ok) throw new Error("Failed to get swarm join token");
				token = result.stdout.trim();
			}
		} else {
			// K3s node token
			const result = await ssh(manager.host, "cat /var/lib/rancher/k3s/server/node-token");
			if (!result.ok) throw new Error("Failed to get K3s node token");
			token = result.stdout.trim();
		}

		log("Testing SSH...");
		const test = await ssh(host, "echo ok");
		if (!test.ok) throw new Error(`SSH failed: ${test.stderr}`);

		const ipResult = await ssh(
			host,
			"curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'",
		);
		const ip = ipResult.stdout.trim();

		await db
			.update(servers)
			.set({ ip, status: "provisioning", runtime })
			.where(eq(servers.id, serverId));

		const { getJoinPhases } = await import("@atlas/provisioner");
		const phases = getJoinPhases({
			runtime,
			managerHost: manager.host,
			managerIp: manager.ip,
			token,
		});

		for (const phase of phases) {
			log(`${phase.name}...`);
			const result = await ssh(host, phase.script);
			if (!result.ok) throw new Error(`${phase.name} failed: ${result.stderr || result.stdout}`);
			log(`${phase.name} ✓`);
		}

		await db
			.update(servers)
			.set({
				status: "online",
				ip,
				meta: {
					provisionedAt: new Date().toISOString(),
					runtime,
					role: "worker",
					managerId: managerServerId,
				},
			})
			.where(eq(servers.id, serverId));

		await db.insert(auditLog).values({
			orgId,
			action: "server.joined",
			resourceType: "server",
			resourceId: serverId,
			meta: { ip, runtime, managerId: managerServerId },
		});

		log("Server joined cluster ✓");
	} catch (e) {
		const error = e instanceof Error ? e.message : String(e);
		log(`FAILED: ${error}`);
		await db
			.update(servers)
			.set({ status: "error", meta: { error } })
			.where(eq(servers.id, serverId));
	}
}
