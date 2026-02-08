import { Elysia } from "elysia"
import { cors } from "@elysiajs/cors"
import { authRoutes } from "./routes/auth"
import { orgRoutes } from "./routes/org"
import { serverRoutes } from "./routes/servers"
import { projectRoutes } from "./routes/projects"
import { deployRoutes } from "./routes/deploys"
import { secretsRoutes } from "./routes/secrets"
import { logsRoutes } from "./routes/logs"

const PORT = Number(process.env.PORT ?? 3100)

const app = new Elysia()
	.use(cors({
		origin: process.env.PANEL_URL ?? "http://localhost:3101",
		credentials: true,
	}))
	.get("/health", () => ({ status: "ok", version: "0.0.1" }))
	.use(authRoutes)
	.use(orgRoutes)
	.use(serverRoutes)
	.use(projectRoutes)
	.use(deployRoutes)
	.use(secretsRoutes)
	.use(logsRoutes)
	.onError(({ error, set }) => {
		const msg = "message" in error ? (error as Error).message : "Internal server error"
		console.error(msg)
		if (msg === "Unauthorized" || msg === "Invalid token") {
			set.status = 401
			return { error: msg }
		}
		if (msg.startsWith("Requires role:")) {
			set.status = 403
			return { error: msg }
		}
		return { error: "Internal server error" }
	})
	.listen(PORT)

console.log(`🔮 Atlas Control Panel API running on http://localhost:${PORT}`)

export type App = typeof app
