import { defineCommand } from "citty"
import { mkdir } from "node:fs/promises"
import { join, basename } from "node:path"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadProject } from "../lib/project"

const templates: Record<string, { description: string; files: Record<string, string> }> = {
	api: {
		description: "Elysia API service",
		files: {
			"package.json": (name: string, orgPrefix: string) => JSON.stringify({
				name: `@${orgPrefix}/${name}`,
				version: "0.0.1",
				scripts: { dev: "bun --watch src/index.ts" },
				dependencies: { elysia: "latest" },
			}, null, 2),
			"src/index.ts": (name: string) => `import { Elysia } from "elysia"

const app = new Elysia({ prefix: "/${name}" })
\t.get("/", () => ({ service: "${name}", status: "ok" }))
\t.get("/health", () => "ok")

export default app
`,
			"tsconfig.json": () => JSON.stringify({
				extends: "../../../tsconfig.json",
				compilerOptions: { outDir: "dist" },
				include: ["src"],
			}, null, 2),
		},
	},
	web: {
		description: "Vite + React SPA",
		files: {
			"package.json": (name: string, orgPrefix: string) => JSON.stringify({
				name: `@${orgPrefix}/${name}`,
				version: "0.0.1",
				scripts: {
					dev: `vite --port 3010`,
					build: "vite build",
					preview: "vite preview",
				},
				dependencies: {
					react: "latest",
					"react-dom": "latest",
					"@tanstack/react-router": "latest",
				},
				devDependencies: {
					vite: "latest",
					"@vitejs/plugin-react": "latest",
				},
			}, null, 2),
			"index.html": (name: string) => `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>${name}</title>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="/src/main.tsx"></script>
</body>
</html>`,
			"src/main.tsx": () => `import { createRoot } from "react-dom/client"

function App() {
\treturn <h1>Hello from Atlas</h1>
}

createRoot(document.getElementById("root")!).render(<App />)
`,
			"vite.config.ts": () => `import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

export default defineConfig({
\tplugins: [react()],
})
`,
		},
	},
	worker: {
		description: "BullMQ worker",
		files: {
			"package.json": (name: string, orgPrefix: string) => JSON.stringify({
				name: `@${orgPrefix}/${name}-worker`,
				version: "0.0.1",
				scripts: { dev: "bun --watch src/index.ts" },
				dependencies: { bullmq: "latest" },
			}, null, 2),
			"src/index.ts": (name: string) => `import { Worker } from "bullmq"

const worker = new Worker(
\t"${name}",
\tasync (job) => {
\t\tconsole.log(\`Processing \${job.name}:\`, job.data)
\t},
\t{ connection: { url: process.env.REDIS_URL } },
)

worker.on("completed", (job) => console.log(\`✓ \${job.id}\`))
worker.on("failed", (job, err) => console.error(\`✗ \${job?.id}:\`, err.message))

console.log(\`Worker "${name}" started\`)
`,
		},
	},
}

export default defineCommand({
	meta: { name: "init", description: "Scaffold a new service" },
	args: {
		type: {
			type: "positional",
			description: "Service type: api, web, worker",
		},
		name: {
			type: "string",
			alias: "n",
			description: "Service name",
		},
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes

		if (!auto) p.intro(pc.bgBlue(pc.black(" atlas init ")))

		// Get project info for dynamic prefix
		const project = await loadProject()
		const orgPrefix = project?.org || basename(process.cwd())

		// Resolve type
		let type = args.type as string
		if (!type && !auto) {
			const selected = await p.select({
				message: "Service type",
				options: Object.entries(templates).map(([key, val]) => ({
					value: key,
					label: `${key} — ${val.description}`,
				})),
			})
			if (p.isCancel(selected)) return p.cancel("Cancelled")
			type = selected as string
		}
		if (!type || !templates[type]) {
			console.error(`Unknown type: ${type}. Options: ${Object.keys(templates).join(", ")}`)
			return
		}

		// Resolve name
		let name = args.name as string
		if (!name && !auto) {
			const input = await p.text({
				message: "Service name",
				placeholder: "my-service",
				validate: (v) => (!v ? "Name is required" : /[^a-z0-9-]/.test(v) ? "Use lowercase + hyphens only" : undefined),
			})
			if (p.isCancel(input)) return p.cancel("Cancelled")
			name = input as string
		}
		if (!name) { console.error("--name is required"); return }

		// Determine output directory
		const baseDirs: Record<string, string> = {
			api: `apps/${name}`,
			web: `apps/web/${name}`,
			worker: `apps/workers/${name}`,
		}
		const dir = baseDirs[type]

		// Create files
		const template = templates[type]
		for (const [filePath, generator] of Object.entries(template.files)) {
			const fullPath = join(dir, filePath)
			await mkdir(join(dir, filePath.includes("/") ? filePath.split("/").slice(0, -1).join("/") : ""), { recursive: true })
			const content = typeof generator === 'function' 
				? generator.length > 1 ? generator(name, orgPrefix) : generator(name)
				: generator
			await Bun.write(fullPath, content)
			console.log(`  ${pc.green("+")} ${fullPath}`)
		}

		if (!auto) {
			p.note(`cd ${dir}\nbun install\nbun run dev`, "Next steps")
			p.outro(pc.green(`${name} created!`))
		} else {
			console.log(`✓ ${name} created at ${dir}`)
		}
	},
})
