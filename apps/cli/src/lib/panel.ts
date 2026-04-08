import type { RouterClient } from "@orpc/server"
import { createORPCClient } from "@orpc/client"
import { RPCLink } from "@orpc/client/fetch"
import type { AppRouter } from "../../panel/src/router"
import { loadConfig } from "./config"

export type PanelClient = RouterClient<AppRouter>

let _client: PanelClient | null = null

export async function createPanelClient(): Promise<PanelClient | null> {
	if (_client) return _client

	const config = await loadConfig()
	if (!config.panelUrl || !config.panelToken) return null

	const link = new RPCLink({
		url: `${config.panelUrl}/rpc`,
		headers: () => ({
			Authorization: `Bearer ${config.panelToken}`,
		}),
	})

	_client = createORPCClient(link)
	return _client
}

// Legacy compat — find project by slug
export async function findProjectBySlug(client: PanelClient, slug: string) {
	const projects = await client.projects.list()
	return projects.find((p: { slug: string }) => p.slug === slug) ?? null
}

// Legacy compat — wait for deploy
export async function waitForDeploy(
	client: PanelClient,
	deployId: number,
	timeoutMs = 180_000,
): Promise<{ status: string }> {
	const start = Date.now()
	while (Date.now() - start < timeoutMs) {
		const deploy = await client.deploys.get({ id: deployId })
		if (deploy && (deploy.status === "success" || deploy.status === "failed" || deploy.status === "rolled_back")) {
			return deploy
		}
		await new Promise((r) => setTimeout(r, 2000))
	}
	return { status: "timeout" }
}
