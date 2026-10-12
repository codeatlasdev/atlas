// The demo stack used by the hero board. Names, ports and worktrees match the real footage in public/media
// (the orbit-shop demo project recorded from Atlas Desktop). The board is a replay of what `atlas dev` prints.

export const YAML = `name: orbit-shop

infra:
  compose_file: docker-compose.yml

services:
  database:
    port: 54320
  migrate:
    command: bun run db:migrate
    oneshot: true
    depends_on: { database: healthy }
  api:
    command: bun run dev:api
    port: 4100
    health: http://localhost:4100/health
    depends_on: { migrate: completed_successfully }
  web:
    command: bun run dev:web
    port: 4101
    depends_on: { api: healthy }
  worker:
    command: bun run dev:worker
    restart: on_failure
    depends_on: { api: healthy }`;

export type NodeId = "database" | "migrate" | "api" | "web" | "worker";

export interface StackNode {
	id: NodeId;
	port?: number;
	/** column in the graph (boot wave) */
	col: number;
	row: number;
	/** seconds into the boot timeline when it reaches healthy */
	note: string;
}

export const NODES: StackNode[] = [
	{ id: "database", port: 54320, col: 0, row: 1, note: "healthy" },
	{ id: "migrate", col: 1, row: 1, note: "exit 0" },
	{ id: "api", port: 4100, col: 2, row: 1, note: "healthy" },
	{ id: "web", port: 4101, col: 3, row: 0, note: "healthy" },
	{ id: "worker", col: 3, row: 2, note: "running" },
];

export const EDGES: [NodeId, NodeId][] = [
	["database", "migrate"],
	["migrate", "api"],
	["api", "web"],
	["api", "worker"],
];

/** The agent worktree forks off and runs its own copy on ports +100 (matches checkout-shipping +100 in the footage). */
export const AGENT = { branch: "agent/checkout-shipping", offset: 100 };

/** Lines that stream under the board. Shapes taken from the real logs capture (shot-logs.jpg). */
export const LOGS: { s: string; t: string; l?: "warn" | "err" }[] = [
	{ s: "api", t: '{"level":30,"msg":"GET /orders","ms":17,"status":200}' },
	{ s: "web", t: "[hmr] updated src/routes/cart.tsx" },
	{ s: "worker", t: "job email.send done in 37ms" },
	{ s: "api", t: '{"level":30,"msg":"POST /checkout","ms":195,"status":201}' },
	{ s: "search", t: 'query "running shoes" 14ms' },
	{ s: "database", t: "checkpoint complete: wrote 42 buffers" },
	{ s: "worker", t: "WARN queue lag 1.2s", l: "warn" },
	{ s: "api", t: '{"level":30,"msg":"GET /orders","ms":60,"status":200}' },
	{ s: "web", t: "[hmr] updated src/routes/cart.tsx" },
	{ s: "worker", t: "job email.send done in 12ms" },
	{ s: "api", t: '{"level":30,"msg":"POST /checkout","ms":87,"status":201}' },
	{ s: "api", t: '{"level":50,"msg":"stripe timeout","ms":5000,"status":504}', l: "err" },
	{ s: "search", t: 'query "running shoes" 14ms' },
	{ s: "worker", t: "job email.send done in 43ms" },
	{ s: "api", t: '{"level":30,"msg":"GET /orders","ms":31,"status":200}' },
	{ s: "web", t: "[hmr] updated src/routes/cart.tsx" },
];
