import { loadConfig } from "./config"

export class PanelClient {
	readonly baseUrl: string

	constructor(
		private url: string,
		private token: string,
	) {
		this.baseUrl = url
	}

	static async create(): Promise<PanelClient | null> {
		const config = await loadConfig()
		if (!config.panelUrl || !config.panelToken) return null
		return new PanelClient(config.panelUrl, config.panelToken)
	}

	async req<T>(path: string, init?: RequestInit): Promise<T> {
		const res = await fetch(`${this.url}${path}`, {
			...init,
			headers: {
				Authorization: `Bearer ${this.token}`,
				"Content-Type": "application/json",
				...init?.headers,
			},
		})

		if (!res.ok) {
			const body = await res.json().catch(() => ({ error: res.statusText })) as { error?: string }
			throw new Error(body.error ?? `HTTP ${res.status}`)
		}

		return res.json() as Promise<T>
	}

	// ── Projects ──

	async listProjects() {
		return this.req<{ id: number; name: string; slug: string; domain: string | null }[]>("/projects")
	}

	async getProject(id: number) {
		return this.req<{
			id: number; name: string; slug: string; domain: string | null
			server: { id: number; name: string; host: string; ip: string | null } | null
			domains: { hostname: string }[]
			deploys: { id: number; tag: string; status: string; startedAt: string }[]
		}>(`/projects/${id}`)
	}

	async findProjectBySlug(slug: string) {
		const projects = await this.listProjects()
		return projects.find((p) => p.slug === slug) ?? null
	}

	// ── Deploys ──

	async triggerDeploy(projectId: number, tag: string, services?: string[]) {
		return this.req<{ id: number; status: string; tag: string }>(`/deploys/project/${projectId}`, {
			method: "POST",
			body: JSON.stringify({ tag, services }),
		})
	}

	async getDeployStatus(deployId: number) {
		return this.req<{ id: number; status: string; tag: string; finishedAt: string | null }>(`/deploys/${deployId}`)
	}

	async waitForDeploy(deployId: number, timeoutMs = 180_000): Promise<{ status: string }> {
		const start = Date.now()
		while (Date.now() - start < timeoutMs) {
			const deploy = await this.getDeployStatus(deployId)
			if (deploy.status === "success" || deploy.status === "failed" || deploy.status === "rolled_back") {
				return deploy
			}
			await new Promise((r) => setTimeout(r, 2000))
		}
		return { status: "timeout" }
	}

	// ── Servers ──

	async listServers() {
		return this.req<{ id: number; name: string; host: string; status: string }[]>("/servers")
	}
}
