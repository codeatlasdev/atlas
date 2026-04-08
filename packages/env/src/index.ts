import { z } from "zod"

function createEnv<T extends z.ZodRawShape>(schema: T): z.infer<z.ZodObject<T>> {
	const parsed = z.object(schema).safeParse(process.env)
	if (!parsed.success) {
		const formatted = parsed.error.flatten().fieldErrors
		const msg = Object.entries(formatted)
			.map(([k, v]) => `  ${k}: ${(v as string[]).join(", ")}`)
			.join("\n")
		throw new Error(`❌ Invalid environment variables:\n${msg}`)
	}
	return parsed.data
}

// ── Panel API env ──

let _panelEnv: ReturnType<typeof createPanelEnv> | null = null

function createPanelEnv() {
	return createEnv({
		PORT: z.coerce.number().default(3100),
		DATABASE_URL: z.string().url().default("postgres://atlas:atlas@localhost:5435/atlas_panel"),
		JWT_SECRET: z.string().min(1).default("atlas-dev-secret-change-in-production"),
		ENCRYPTION_KEY: z.string().min(1).default("atlas-dev-secret-change-in-production"),
		PANEL_URL: z.string().optional(),
		GITHUB_CLIENT_ID: z.string().optional(),
		GITHUB_CLIENT_SECRET: z.string().optional(),
		NODE_ENV: z.enum(["development", "production"]).default("development"),
	})
}

export function panelEnv() {
	if (!_panelEnv) _panelEnv = createPanelEnv()
	return _panelEnv
}

export type PanelEnv = ReturnType<typeof panelEnv>
