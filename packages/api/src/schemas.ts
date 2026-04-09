import { z } from "zod";

// ── Shared schemas ──

export const ServerSchema = z.object({
	id: z.number(),
	name: z.string(),
	host: z.string(),
	ip: z.string().nullable(),
	runtime: z.enum(["k3s", "swarm"]),
	status: z.enum(["provisioning", "online", "offline", "error"]),
});

export const ProjectSchema = z.object({
	id: z.number(),
	name: z.string(),
	slug: z.string(),
	domain: z.string().nullable(),
});

export const DeploySchema = z.object({
	id: z.number(),
	tag: z.string(),
	status: z.enum([
		"pending",
		"building",
		"pushing",
		"deploying",
		"success",
		"failed",
		"rolled_back",
	]),
	startedAt: z.string(),
	finishedAt: z.string().nullable(),
});

export const OrgSchema = z.object({
	id: z.number(),
	name: z.string(),
	slug: z.string(),
	githubOrg: z.string(),
	cloudflareConfigured: z.boolean(),
	cloudflareAccountId: z.string().nullable(),
	githubAppConfigured: z.boolean(),
	githubAppId: z.number().nullable(),
});
