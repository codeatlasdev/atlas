import { os } from "@orpc/server"
import { z } from "zod"
import type { AuthContext } from "@atlas/auth"

// ── Base with auth context ──

const authed = os.$context<{ auth: AuthContext }>()

// ── Org ──

const org = {
	get: authed.handler(async ({ context }) => {
		return context // placeholder — implemented in panel
	}),
	updateSettings: authed
		.input(
			z.object({
				cloudflareToken: z.string().optional(),
				cloudflareAccountId: z.string().optional(),
				githubToken: z.string().optional(),
				githubAppId: z.number().optional(),
				githubClientId: z.string().optional(),
				githubClientSecret: z.string().optional(),
			}),
		)
		.handler(async ({ input, context }) => {
			return { ok: true } as const
		}),
}

// ── Servers ──

const servers = {
	list: authed.handler(async () => {
		return [] as { id: number; name: string; host: string; ip: string | null; status: string }[]
	}),
	get: authed.input(z.object({ id: z.number() })).handler(async ({ input }) => {
		return null as { id: number; name: string; host: string; ip: string | null; status: string } | null
	}),
	create: authed
		.input(
			z.object({
				name: z.string(),
				host: z.string(),
				ip: z.string().optional(),
				provision: z.boolean().optional(),
				domain: z.string().optional(),
			}),
		)
		.handler(async ({ input }) => {
			return {} as { id: number; name: string; host: string; status: string }
		}),
	update: authed
		.input(
			z.object({
				id: z.number(),
				kubeconfig: z.string().optional(),
				status: z.string().optional(),
				ip: z.string().optional(),
			}),
		)
		.handler(async ({ input }) => {
			return {} as { id: number }
		}),
	delete: authed.input(z.object({ id: z.number() })).handler(async () => {
		return { ok: true } as const
	}),
}

// ── Projects ──

const projects = {
	list: authed.handler(async () => {
		return [] as { id: number; name: string; slug: string; domain: string | null }[]
	}),
	get: authed.input(z.object({ id: z.number() })).handler(async ({ input }) => {
		return null as unknown
	}),
	create: authed
		.input(
			z.object({
				name: z.string(),
				serverId: z.number().optional(),
				githubRepo: z.string().optional(),
				domain: z.string().optional(),
				atlasYaml: z.string().optional(),
			}),
		)
		.handler(async ({ input }) => {
			return {} as { id: number; name: string; slug: string }
		}),
	update: authed
		.input(
			z.object({
				id: z.number(),
				serverId: z.number().optional(),
				domain: z.string().optional(),
				atlasYaml: z.string().optional(),
			}),
		)
		.handler(async ({ input }) => {
			return {} as { id: number }
		}),
}

// ── Deploys ──

const deploys = {
	listByProject: authed.input(z.object({ projectId: z.number() })).handler(async () => {
		return [] as { id: number; tag: string; status: string; startedAt: string }[]
	}),
	trigger: authed
		.input(
			z.object({
				projectId: z.number(),
				tag: z.string(),
				services: z.array(z.string()).optional(),
			}),
		)
		.handler(async ({ input }) => {
			return {} as { id: number; status: string; tag: string }
		}),
	get: authed.input(z.object({ id: z.number() })).handler(async ({ input }) => {
		return null as { id: number; status: string; tag: string; finishedAt: string | null } | null
	}),
}

// ── Secrets ──

const secrets = {
	listKeys: authed.input(z.object({ projectId: z.number() })).handler(async () => {
		return [] as { key: string; updatedAt: string }[]
	}),
	set: authed
		.input(
			z.object({
				projectId: z.number(),
				secrets: z.record(z.string(), z.string()),
			}),
		)
		.handler(async ({ input }) => {
			return { ok: true, keys: [] as string[], synced: false }
		}),
	delete: authed
		.input(z.object({ projectId: z.number(), key: z.string() }))
		.handler(async ({ input }) => {
			return { ok: true, deleted: input.key, synced: false }
		}),
	pullValues: authed.input(z.object({ projectId: z.number() })).handler(async () => {
		return {} as Record<string, string>
	}),
}

// ── Router ──

export const router = os.router({
	org,
	servers,
	projects,
	deploys,
	secrets,
})

export type Router = typeof router
