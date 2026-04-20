import { parse } from "yaml";

export interface ServiceConfig {
	type: "api" | "web" | "worker";
	dockerfile: string;
	target?: string;
	buildArg?: string;
	port?: number;
	replicas?: number;
	health?: string;
	domain?: string;
}

export interface ProjectConfig {
	name: string;
	org: string;
	domain?: string;
	platform?: string;
	services: Record<string, ServiceConfig>;
	infra?: {
		postgres?: boolean;
		redis?: boolean;
		tunnel?: boolean; // Cloudflare Tunnel
	};
}

const FILENAME = "atlas.yaml";

export async function loadProject(dir?: string): Promise<ProjectConfig | null> {
	const path = `${dir || process.cwd()}/${FILENAME}`;
	const file = Bun.file(path);
	if (!(await file.exists())) return null;
	return parse(await file.text()) as ProjectConfig;
}

export async function saveProject(config: ProjectConfig, dir?: string): Promise<void> {
	const { stringify } = await import("yaml");
	const path = `${dir || process.cwd()}/${FILENAME}`;
	await Bun.write(path, stringify(config));
}

/** Resolve the K8s namespace / Swarm stack name for the current project. */
export async function resolveNamespace(configDomain?: string): Promise<string> {
	const project = await loadProject();
	if (project) return project.name;
	return configDomain?.split(".")[0] ?? "app";
}
