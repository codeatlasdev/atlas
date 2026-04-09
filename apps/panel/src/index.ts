import { panelEnv } from "@atlas/env";
import { cors } from "@elysiajs/cors";
import { onError } from "@orpc/server";
import { RPCHandler } from "@orpc/server/fetch";
import { Elysia } from "elysia";
import { router } from "./router";
import { authRoutes } from "./routes/auth";
import { logsRoutes } from "./routes/logs";

const env = panelEnv();

const handler = new RPCHandler(router, {
	interceptors: [
		onError((error) => {
			console.error(error);
		}),
	],
});

const app = new Elysia()
	.use(
		cors({
			origin: env.PANEL_URL ?? "http://localhost:3101",
			credentials: true,
		}),
	)
	.get("/health", () => ({ status: "ok", version: "0.1.0" }))
	.all(
		"/rpc/*",
		async ({ request }) => {
			const { response } = await handler.handle(request, {
				prefix: "/rpc",
				context: { request },
			});
			return response ?? new Response("Not Found", { status: 404 });
		},
		{ parse: "none" },
	)
	.use(authRoutes)
	.use(logsRoutes)
	.onError(({ error, set }) => {
		const msg = "message" in error ? (error as Error).message : "Internal server error";
		console.error(msg);
		if (msg === "Unauthorized" || msg === "Invalid token") {
			set.status = 401;
			return { error: msg };
		}
		if (msg.startsWith("Requires role:")) {
			set.status = 403;
			return { error: msg };
		}
		return { error: "Internal server error" };
	})
	.listen(env.PORT);

console.log(`🔮 Atlas Control Panel API running on http://localhost:${env.PORT}`);

export type App = typeof app;
