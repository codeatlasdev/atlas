import { drizzle } from "drizzle-orm/postgres-js"
import postgres from "postgres"
import * as schema from "./schema"

const connectionString = process.env.DATABASE_URL ?? "postgresql://atlas:atlas@localhost:5432/atlas_panel"

const client = postgres(connectionString)
export const db = drizzle(client, { schema, casing: "snake_case" })
export type Database = typeof db
