import { CloudflareClient } from "@atlas/cloudflare";
import { db } from "@atlas/db";
import { deploys, domains, organizations, projects } from "@atlas/db/schema";
import { createRuntime } from "@atlas/runtime";
import { eq } from "drizzle-orm";
import { parse } from "yaml";

interface ServiceConfig {
	type: "api" | "web" | "worker";
	dockerfile: string;
	target?: string;
	port?: number;
	domain?: string;
}

interface ProjectConfig {
	name: string;
	org: string;
	domain?: string;
	services: Record<string, ServiceConfig>;
	infra?: { postgres?: boolean; redis?: boolean; tunnel?: boolean };
}

export async function executeDeploy(deployId: number): Promise<void> {
	const deploy = await db.query.deploys.findFirst({
		where: eq(deploys.id, deployId),
		with: { project: { with: { server: true } } },
	});

	if (!deploy?.project?.server) {
		await updateStatus(deployId, "failed", { error: "No server assigned" });
		return;
	}

	const project = deploy.project;
	const server = deploy.project.server!;

	if (!server.host) {
		await updateStatus(deployId, "failed", { error: "Server has no host" });
		return;
	}

	let config: ProjectConfig;
	if (project.atlasYaml) {
		config = parse(project.atlasYaml) as ProjectConfig;
	} else {
		await updateStatus(deployId, "failed", { error: "Project has no atlas.yaml" });
		return;
	}

	const registry = `ghcr.io/${config.org}/${config.name}`;
	const ns = config.name;
	const tag = deploy.tag;
	const runtime = createRuntime(server.runtime, server.host);

	try {
		await updateStatus(deployId, "deploying");

		const meta = deploy.meta as { services?: string[] } | null;
		let serviceEntries = Object.entries(config.services);
		if (meta?.services?.length) {
			serviceEntries = serviceEntries.filter(([name]) => meta.services!.includes(name));
		}

		// Migrations
		const hasMigrate = serviceEntries.some(([name]) => name === "migrate");
		if (hasMigrate) {
			const migrateImage = `${registry}/migrate:${tag}`;
			await runtime.runJob(ns, "migrate", migrateImage, `${ns}-secrets`);
		}

		// Deploy services
		const deployable = serviceEntries.filter(([name]) => name !== "migrate");
		for (const [name] of deployable) {
			await runtime.deploy(ns, name, `${registry}/${name}:${tag}`);
		}

		// Wait for rollout
		let allHealthy = true;
		for (const [name] of deployable) {
			const ok = await runtime.rolloutStatus(ns, name);
			if (!ok) {
				allHealthy = false;
				console.error(`Rollout failed for ${name}`);
			}
		}

		if (!allHealthy) {
			await updateStatus(deployId, "failed", { error: "Rollout failed" });
			return;
		}

		// DNS
		const org = await db.query.organizations.findFirst({
			where: eq(organizations.id, project.orgId),
		});

		if (org?.cloudflareTokenEnc && org?.cloudflareAccountId && server.ip) {
			const { decrypt } = await import("@atlas/crypto");
			const cfToken = await decrypt(org.cloudflareTokenEnc);
			const cf = new CloudflareClient(cfToken, org.cloudflareAccountId);
			const domainsToSetup = serviceEntries
				.filter(([, svc]) => svc.domain)
				.map(([, svc]) => svc.domain!);

			if (config.domain && !domainsToSetup.includes(config.domain)) {
				domainsToSetup.push(config.domain);
			}

			for (const hostname of domainsToSetup) {
				try {
					const result = await cf.ensureDNS(hostname, server.ip);
					const existing = await db
						.select()
						.from(domains)
						.where(eq(domains.hostname, hostname))
						.limit(1);

					if (existing.length > 0) {
						await db
							.update(domains)
							.set({ dnsRecordId: result.recordId, verified: true })
							.where(eq(domains.hostname, hostname));
					} else {
						await db.insert(domains).values({
							projectId: project.id,
							hostname,
							dnsRecordId: result.recordId,
							verified: true,
						});
					}
				} catch (e) {
					console.error(`DNS failed for ${hostname}:`, e);
				}
			}
		}

		await updateStatus(deployId, "success");
	} catch (e) {
		await updateStatus(deployId, "failed", {
			error: e instanceof Error ? e.message : String(e),
		});
	}
}

async function updateStatus(
	deployId: number,
	status: "pending" | "building" | "pushing" | "deploying" | "success" | "failed" | "rolled_back",
	meta?: Record<string, unknown>,
) {
	await db
		.update(deploys)
		.set({
			status,
			...(meta ? { meta } : {}),
			...(status === "success" || status === "failed" ? { finishedAt: new Date() } : {}),
		})
		.where(eq(deploys.id, deployId));
}
