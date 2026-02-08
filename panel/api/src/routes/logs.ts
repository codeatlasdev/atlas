import { Elysia } from "elysia"
import { eq } from "drizzle-orm"
import { db } from "../db"
import { projects } from "../db/schema"
import { requireAuth } from "../lib/guard"
import { KubernetesService } from "../services/kubernetes"

export const logsRoutes = new Elysia({ prefix: "/logs" })
	.get("/project/:projectId", async ({ headers, params, query }) => {
		await requireAuth(headers.authorization)

		const project = await db.query.projects.findFirst({
			where: eq(projects.id, Number(params.projectId)),
			with: { server: true },
		})
		if (!project?.server?.host) return new Response("Project or server not found", { status: 404 })

		const service = (query as Record<string, string>).service
		if (!service) return new Response("Missing ?service= param", { status: 400 })

		const tail = Number((query as Record<string, string>).tail) || 100
		const follow = (query as Record<string, string>).follow === "true"

		const kube = new KubernetesService(project.server.host)
		const stream = await kube.streamLogs(project.slug, service, { tail, follow })

		// Transform to SSE format
		const encoder = new TextEncoder()
		const sse = new TransformStream<Uint8Array, Uint8Array>({
			transform(chunk, controller) {
				const text = new TextDecoder().decode(chunk)
				for (const line of text.split("\n")) {
					if (line) controller.enqueue(encoder.encode(`data: ${line}\n\n`))
				}
			},
		})

		return new Response(stream.pipeThrough(sse), {
			headers: {
				"Content-Type": "text/event-stream",
				"Cache-Control": "no-cache",
				Connection: "keep-alive",
			},
		})
	})
