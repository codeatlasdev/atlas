import { defineCommand } from "citty"
import { $ } from "bun"
import { mkdir } from "node:fs/promises"
import { join } from "node:path"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { loadConfig } from "../lib/config"
import { saveProject, type ProjectConfig } from "../lib/project"

export default defineCommand({
	meta: { name: "create", description: "Create a new project with full infrastructure" },
	args: {
		name: { type: "positional", description: "Project name" },
		type: { type: "string", alias: "t", description: "Project type: api, web, fullstack" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const auto = args.yes
		const config = await loadConfig()
		const org = config.githubUser || "codeatlasdev"

		if (!auto) p.intro(pc.bgBlue(pc.black(" atlas create ")))

		// Name
		let name = args.name as string
		if (!name && !auto) {
			const input = await p.text({
				message: "Project name",
				placeholder: "my-api",
				validate: (v) => (!v ? "Required" : /[^a-z0-9-]/.test(v) ? "Lowercase + hyphens only" : undefined),
			})
			if (p.isCancel(input)) return p.cancel("Cancelled")
			name = input as string
		}
		if (!name) { console.error("Name required"); return }

		// Type
		let type = args.type as string
		if (!type && !auto) {
			const selected = await p.select({
				message: "Project type",
				options: [
					{ value: "api", label: "API — Elysia backend with Postgres + Redis" },
					{ value: "web", label: "Web — Vite + React SPA" },
					{ value: "fullstack", label: "Fullstack — API + Web monorepo" },
				],
			})
			if (p.isCancel(selected)) return p.cancel("Cancelled")
			type = selected as string
		}
		type = type || "api"

		// Domain
		const baseDomain = config.domain || "codeatlas.dev"
		const domain = `${name}.${baseDomain}`

		const dir = join(process.cwd(), name)

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner()

		// 1. Create directory + scaffold
		log.start("Scaffolding project...")
		await mkdir(join(dir, "src"), { recursive: true })
		await mkdir(join(dir, "infra/docker"), { recursive: true })
		await mkdir(join(dir, ".github/workflows"), { recursive: true })

		// atlas.yaml
		const project: ProjectConfig = {
			name,
			org,
			domain,
			services: {},
			infra: {},
		}

		if (type === "api" || type === "fullstack") {
			project.services.server = {
				type: "api",
				dockerfile: "Dockerfile",
				port: 3001,
				replicas: 1,
				health: "/health",
				domain: `api.${domain}`,
			}
			project.infra!.postgres = true
			project.infra!.redis = true

			// package.json
			await Bun.write(join(dir, "package.json"), JSON.stringify({
				name: `@${org}/${name}`,
				version: "0.0.1",
				scripts: { dev: "bun --watch src/index.ts", start: "bun src/index.ts" },
				dependencies: { elysia: "latest" },
			}, null, 2))

			// src/index.ts
			await Bun.write(join(dir, "src/index.ts"), `import { Elysia } from "elysia"

const app = new Elysia()
\t.get("/", () => ({ name: "${name}", status: "ok" }))
\t.get("/health", () => "ok")
\t.listen(process.env.PORT || 3001)

console.log(\`🚀 ${name} running at http://localhost:\${app.server?.port}\`)
`)

			// Dockerfile
			await Bun.write(join(dir, "Dockerfile"), `FROM oven/bun:1-slim
WORKDIR /app
COPY package.json bun.lock* ./
RUN bun install --production
COPY . .
ENV NODE_ENV=production
EXPOSE 3001
HEALTHCHECK --interval=30s --timeout=3s CMD bun -e "fetch('http://localhost:3001/health').then(r=>r.ok?process.exit(0):process.exit(1)).catch(()=>process.exit(1))"
CMD ["bun", "src/index.ts"]
`)
		}

		if (type === "web" || type === "fullstack") {
			const webName = type === "fullstack" ? "web" : "app"
			project.services[webName] = {
				type: "web",
				dockerfile: type === "fullstack" ? "infra/docker/Dockerfile.web" : "Dockerfile",
				domain,
			}

			if (type === "web") {
				await Bun.write(join(dir, "package.json"), JSON.stringify({
					name: `@${org}/${name}`,
					version: "0.0.1",
					scripts: { dev: "vite", build: "vite build", preview: "vite preview" },
					dependencies: { react: "latest", "react-dom": "latest" },
					devDependencies: { vite: "latest", "@vitejs/plugin-react": "latest" },
				}, null, 2))
			}
		}

		project.infra!.tunnel = true
		await saveProject(project, dir)
		log.stop("Project scaffolded")

		// 2. Git init
		log.start("Initializing git...")
		await Bun.write(join(dir, ".gitignore"), "node_modules\ndist\n.env\n.env.local\n")
		await $`cd ${dir} && git init -b main && git add -A && git commit -m "feat: initial scaffold via atlas create"`.quiet()
		log.stop("Git initialized")

		// 3. Create GitHub repo
		log.start(`Creating github.com/${org}/${name}...`)
		try {
			await $`gh repo create ${org}/${name} --private --source=${dir} --push`.quiet()
			log.stop(`github.com/${org}/${name} created`)
		} catch {
			log.stop("GitHub repo creation skipped (gh not available or repo exists)")
			// Still try to push
			try {
				await $`cd ${dir} && git remote add origin https://github.com/${org}/${name}.git && git push -u origin main`.quiet()
			} catch {}
		}

		// 4. CI/CD workflow
		await Bun.write(join(dir, ".github/workflows/deploy.yml"), `name: Deploy
on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - run: bun install
      - name: Install Atlas
        run: curl -sfL https://raw.githubusercontent.com/codeatlasdev/atlas/main/install.sh | sh
      - name: Deploy
        run: atlas deploy -y
        env:
          GITHUB_TOKEN: \${{ secrets.GITHUB_TOKEN }}
`)

		if (!auto) {
			p.note([
				`cd ${name}`,
				"bun install",
				"bun run dev",
				"",
				"# When ready:",
				"atlas deploy",
			].join("\n"), "Next steps")
			p.outro(pc.green(`${name} created! 🚀`))
		} else {
			console.log(`✓ ${name} created at ./${name}`)
		}
	},
})
