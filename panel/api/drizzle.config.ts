import { defineConfig } from "drizzle-kit"

export default defineConfig({
	schema: "./src/db/schema.ts",
	out: "./drizzle",
	dialect: "postgresql",
	dbCredentials: {
		url: process.env.DATABASE_URL ?? "postgresql://atlas:atlas@localhost:5432/atlas_panel",
	},
	casing: "snake_case",
})
