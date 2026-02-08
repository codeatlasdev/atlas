import { relations } from "drizzle-orm"
import {
	boolean,
	index,
	integer,
	jsonb,
	pgEnum,
	pgTable,
	text,
	timestamp,
	uniqueIndex,
	varchar,
} from "drizzle-orm/pg-core"

// ── Enums ──

export const userRoleEnum = pgEnum("user_role", ["admin", "dev", "viewer"])
export const serverStatusEnum = pgEnum("server_status", ["provisioning", "online", "offline", "error"])
export const deployStatusEnum = pgEnum("deploy_status", ["pending", "building", "pushing", "deploying", "success", "failed", "rolled_back"])
export const previewStatusEnum = pgEnum("preview_status", ["creating", "running", "destroying", "destroyed"])

// ── Organizations ──

export const organizations = pgTable("organizations", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	name: varchar({ length: 255 }).notNull(),
	slug: varchar({ length: 255 }).notNull(),
	githubOrg: varchar("github_org", { length: 255 }).notNull(),
	cloudflareTokenEnc: text("cloudflare_token_enc"),
	cloudflareAccountId: varchar("cloudflare_account_id", { length: 255 }),
	githubTokenEnc: text("github_token_enc"),
	githubAppId: integer("github_app_id"),
	githubClientId: varchar("github_client_id", { length: 255 }),
	githubClientSecretEnc: text("github_client_secret_enc"),
	createdAt: timestamp("created_at").defaultNow().notNull(),
}, (t) => [
	uniqueIndex("org_slug_idx").on(t.slug),
])

// ── Users ──

export const users = pgTable("users", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	githubId: integer("github_id").notNull(),
	githubUsername: varchar("github_username", { length: 255 }).notNull(),
	avatarUrl: text("avatar_url"),
	email: varchar({ length: 255 }),
	role: userRoleEnum().default("dev").notNull(),
	orgId: integer("org_id").references(() => organizations.id).notNull(),
	createdAt: timestamp("created_at").defaultNow().notNull(),
}, (t) => [
	uniqueIndex("user_github_id_idx").on(t.githubId),
])

// ── Servers ──

export const servers = pgTable("servers", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	name: varchar({ length: 255 }).notNull(),
	host: varchar({ length: 255 }).notNull(),
	ip: varchar({ length: 45 }),
	kubeconfigEnc: text("kubeconfig_enc"),
	status: serverStatusEnum().default("offline").notNull(),
	orgId: integer("org_id").references(() => organizations.id).notNull(),
	meta: jsonb(),
	createdAt: timestamp("created_at").defaultNow().notNull(),
}, (t) => [
	uniqueIndex("server_name_org_idx").on(t.name, t.orgId),
])

// ── Projects ──

export const projects = pgTable("projects", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	name: varchar({ length: 255 }).notNull(),
	slug: varchar({ length: 255 }).notNull(),
	orgId: integer("org_id").references(() => organizations.id).notNull(),
	serverId: integer("server_id").references(() => servers.id),
	githubRepo: varchar("github_repo", { length: 255 }),
	domain: varchar({ length: 255 }),
	atlasYaml: text("atlas_yaml"),
	createdAt: timestamp("created_at").defaultNow().notNull(),
}, (t) => [
	uniqueIndex("project_slug_org_idx").on(t.slug, t.orgId),
])

// ── Deploys ──

export const deploys = pgTable("deploys", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	projectId: integer("project_id").references(() => projects.id).notNull(),
	userId: integer("user_id").references(() => users.id).notNull(),
	tag: varchar({ length: 255 }).notNull(),
	status: deployStatusEnum().default("pending").notNull(),
	meta: jsonb(),
	startedAt: timestamp("started_at").defaultNow().notNull(),
	finishedAt: timestamp("finished_at"),
}, (t) => [
	index("deploy_project_idx").on(t.projectId),
])

// ── Secrets ──

export const secrets = pgTable("secrets", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	projectId: integer("project_id").references(() => projects.id).notNull(),
	key: varchar({ length: 255 }).notNull(),
	valueEnc: text("value_enc").notNull(),
	updatedAt: timestamp("updated_at").defaultNow().notNull(),
}, (t) => [
	uniqueIndex("secret_project_key_idx").on(t.projectId, t.key),
])

// ── Domains ──

export const domains = pgTable("domains", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	projectId: integer("project_id").references(() => projects.id).notNull(),
	hostname: varchar({ length: 255 }).notNull(),
	dnsRecordId: varchar("dns_record_id", { length: 255 }),
	zoneId: varchar("zone_id", { length: 255 }),
	verified: boolean().default(false).notNull(),
	createdAt: timestamp("created_at").defaultNow().notNull(),
}, (t) => [
	uniqueIndex("domain_hostname_idx").on(t.hostname),
])

// ── Previews ──

export const previews = pgTable("previews", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	projectId: integer("project_id").references(() => projects.id).notNull(),
	prNumber: integer("pr_number"),
	namespace: varchar({ length: 255 }).notNull(),
	url: text(),
	status: previewStatusEnum().default("creating").notNull(),
	createdAt: timestamp("created_at").defaultNow().notNull(),
	expiresAt: timestamp("expires_at"),
}, (t) => [
	index("preview_project_idx").on(t.projectId),
])

// ── Audit Log ──

export const auditLog = pgTable("audit_log", {
	id: integer().primaryKey().generatedAlwaysAsIdentity(),
	orgId: integer("org_id").references(() => organizations.id).notNull(),
	userId: integer("user_id").references(() => users.id),
	action: varchar({ length: 255 }).notNull(),
	resourceType: varchar("resource_type", { length: 100 }).notNull(),
	resourceId: integer("resource_id"),
	meta: jsonb(),
	createdAt: timestamp("created_at").defaultNow().notNull(),
}, (t) => [
	index("audit_org_idx").on(t.orgId),
	index("audit_created_idx").on(t.createdAt),
])

// ── Relations ──

export const organizationsRelations = relations(organizations, ({ many }) => ({
	users: many(users),
	servers: many(servers),
	projects: many(projects),
}))

export const usersRelations = relations(users, ({ one, many }) => ({
	org: one(organizations, { fields: [users.orgId], references: [organizations.id] }),
	deploys: many(deploys),
}))

export const serversRelations = relations(servers, ({ one, many }) => ({
	org: one(organizations, { fields: [servers.orgId], references: [organizations.id] }),
	projects: many(projects),
}))

export const projectsRelations = relations(projects, ({ one, many }) => ({
	org: one(organizations, { fields: [projects.orgId], references: [organizations.id] }),
	server: one(servers, { fields: [projects.serverId], references: [servers.id] }),
	deploys: many(deploys),
	secrets: many(secrets),
	domains: many(domains),
	previews: many(previews),
}))

export const deploysRelations = relations(deploys, ({ one }) => ({
	project: one(projects, { fields: [deploys.projectId], references: [projects.id] }),
	user: one(users, { fields: [deploys.userId], references: [users.id] }),
}))

export const secretsRelations = relations(secrets, ({ one }) => ({
	project: one(projects, { fields: [secrets.projectId], references: [projects.id] }),
}))

export const domainsRelations = relations(domains, ({ one }) => ({
	project: one(projects, { fields: [domains.projectId], references: [projects.id] }),
}))

export const previewsRelations = relations(previews, ({ one }) => ({
	project: one(projects, { fields: [previews.projectId], references: [projects.id] }),
}))
