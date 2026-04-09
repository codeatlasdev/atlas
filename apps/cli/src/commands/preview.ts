import { ssh } from "@atlas/ssh";
import * as p from "@clack/prompts";
import { $ } from "bun";
import { defineCommand } from "citty";
import pc from "picocolors";
import { loadConfig } from "../lib/config";

function k3sPreviewDeploy(ns: string, name: string, domain: string, repo: string, tag: string) {
	const mainNs = domain.split(".")[0];
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace ${ns} --dry-run=client -o yaml | kubectl apply -f -
MAIN_NS="${mainNs}"
kubectl -n \${MAIN_NS} get secret \${MAIN_NS}-secrets -o yaml | sed "s/namespace: \${MAIN_NS}/namespace: ${ns}/" | kubectl apply -f -
cat <<YAML | kubectl apply -f -
apiVersion: apps/v1
kind: Deployment
metadata:
  name: server
  namespace: ${ns}
spec:
  replicas: 1
  selector:
    matchLabels: { app: server }
  template:
    metadata:
      labels: { app: server }
    spec:
      containers:
        - name: server
          image: ghcr.io/${repo}/server:${tag}
          ports: [{ containerPort: 3001 }]
          envFrom: [{ secretRef: { name: \${MAIN_NS}-secrets } }]
---
apiVersion: v1
kind: Service
metadata: { name: server, namespace: ${ns} }
spec:
  selector: { app: server }
  ports: [{ port: 3001, targetPort: 3001 }]
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: backoffice
  namespace: ${ns}
spec:
  replicas: 1
  selector:
    matchLabels: { app: backoffice }
  template:
    metadata:
      labels: { app: backoffice }
    spec:
      containers:
        - name: backoffice
          image: ghcr.io/${repo}/backoffice:${tag}
          ports: [{ containerPort: 80 }]
---
apiVersion: v1
kind: Service
metadata: { name: backoffice, namespace: ${ns} }
spec:
  selector: { app: backoffice }
  ports: [{ port: 80, targetPort: 80 }]
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: preview
  namespace: ${ns}
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
spec:
  rules:
    - host: ${name}.preview.${domain}
      http:
        paths:
          - path: /api
            pathType: Prefix
            backend: { service: { name: server, port: { number: 3001 } } }
          - path: /
            pathType: Prefix
            backend: { service: { name: backoffice, port: { number: 80 } } }
  tls:
    - hosts: ["${name}.preview.${domain}"]
      secretName: preview-${name}-tls
YAML
echo "ok"`;
}

function swarmPreviewDeploy(ns: string, name: string, domain: string, repo: string, tag: string) {
	return `set -euo pipefail
cat > /tmp/${ns}-compose.yml <<'COMPOSE'
services:
  server:
    image: ghcr.io/${repo}/server:${tag}
    networks:
      - default
      - atlas-proxy
    deploy:
      labels:
        - "traefik.enable=true"
        - "traefik.http.routers.${ns}-api.rule=Host(\`${name}.preview.${domain}\`) && PathPrefix(\`/api\`)"
        - "traefik.http.routers.${ns}-api.tls.certresolver=le"
        - "traefik.http.services.${ns}-api.loadbalancer.server.port=3001"

  backoffice:
    image: ghcr.io/${repo}/backoffice:${tag}
    networks:
      - default
      - atlas-proxy
    deploy:
      labels:
        - "traefik.enable=true"
        - "traefik.http.routers.${ns}-web.rule=Host(\`${name}.preview.${domain}\`)"
        - "traefik.http.routers.${ns}-web.tls.certresolver=le"
        - "traefik.http.services.${ns}-web.loadbalancer.server.port=80"

networks:
  atlas-proxy:
    external: true
COMPOSE

docker stack deploy -c /tmp/${ns}-compose.yml ${ns}
rm -f /tmp/${ns}-compose.yml
echo "ok"`;
}

const start = defineCommand({
	meta: { name: "start", description: "Create a preview environment from current code" },
	args: {
		name: { type: "string", description: "Preview name (default: branch name)" },
		host: { type: "string", description: "SSH host" },
		yes: { type: "boolean", alias: "y", default: false },
	},
	async run({ args }) {
		const config = await loadConfig();
		const host = args.host || config.host;
		const domain = config.domain;
		const repo = config.githubRepo;
		const runtime = config.runtime || "k3s";
		if (!host) {
			console.error("No host. Run: atlas infra setup");
			return;
		}
		if (!domain) {
			console.error("No domain configured. Run: atlas infra setup");
			return;
		}
		if (!repo) {
			console.error("No repo configured. Run: atlas init");
			return;
		}

		const auto = args.yes;
		const branch = (await $`git branch --show-current`.quiet()).stdout.toString().trim();
		const name = args.name || branch.replace(/[^a-z0-9-]/g, "-").slice(0, 30);
		const ns = `preview-${name}`;
		const tag = `preview-${name}-${Date.now()}`;
		const previewUrl = `https://${name}.preview.${domain}`;

		if (!auto) {
			p.intro(pc.bgYellow(pc.black(" atlas preview ")));
			const proceed = await p.confirm({
				message: `Create preview ${pc.bold(name)} at ${pc.cyan(previewUrl)}? (${runtime})`,
			});
			if (p.isCancel(proceed) || !proceed) return p.cancel("Cancelled");
		}

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner();

		// Build
		const images = [
			{
				name: `ghcr.io/${repo}/server`,
				dockerfile: "infra/docker/Dockerfile.api",
				target: "server",
			},
			{
				name: `ghcr.io/${repo}/backoffice`,
				dockerfile: "infra/docker/Dockerfile.web",
				buildArg: "APP_NAME=backoffice",
			},
		];

		for (const img of images) {
			const short = img.name.split("/").pop();
			log.start(`Building ${short}`);
			const buildArgs = ["docker", "build", "-f", img.dockerfile, "-t", `${img.name}:${tag}`];
			if (img.target) buildArgs.push("--target", img.target);
			if (img.buildArg) buildArgs.push("--build-arg", img.buildArg);
			buildArgs.push(".");

			const build = Bun.spawn(buildArgs, { stdout: "pipe", stderr: "pipe" });
			if ((await build.exited) !== 0) {
				log.stop(`${short} — FAILED`);
				console.error(await new Response(build.stderr).text());
				return;
			}
			log.stop(`${short} built`);
		}

		// Push
		log.start("Pushing images...");
		for (const img of images) {
			const push = Bun.spawn(["docker", "push", `${img.name}:${tag}`], {
				stdout: "pipe",
				stderr: "pipe",
			});
			if ((await push.exited) !== 0) {
				log.stop("Push failed");
				return;
			}
		}
		log.stop("Images pushed");

		// Deploy
		log.start("Creating preview environment...");
		const script =
			runtime === "swarm"
				? swarmPreviewDeploy(ns, name, domain, repo, tag)
				: k3sPreviewDeploy(ns, name, domain, repo, tag);

		const result = await ssh(host, script);
		if (!result.ok) {
			log.stop("Preview creation failed");
			console.error(result.stderr || result.stdout);
			return;
		}
		log.stop("Preview environment created");

		if (!auto) {
			p.note(
				`${pc.cyan(previewUrl)}\n\nStop with: ${pc.dim(`atlas preview stop --name ${name}`)}`,
				"Preview URL",
			);
			p.outro(pc.green("Preview is deploying!"));
		} else {
			console.log(`✓ Preview: ${previewUrl}`);
			console.log(`  Stop: atlas preview stop --name ${name}`);
		}
	},
});

const stop = defineCommand({
	meta: { name: "stop", description: "Destroy a preview environment" },
	args: {
		name: { type: "string", description: "Preview name", required: true },
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig();
		const host = args.host || config.host;
		const runtime = config.runtime || "k3s";
		if (!host) {
			console.error("No host. Run: atlas infra setup");
			return;
		}

		const name = args.name as string;
		const ns = `preview-${name}`;

		console.log(`→ Destroying preview ${name}...`);
		const cmd =
			runtime === "swarm"
				? `docker stack rm ${ns} 2>/dev/null; echo "ok"`
				: `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; kubectl delete namespace ${ns} --ignore-not-found`;

		const result = await ssh(host, cmd);
		if (!result.ok) {
			console.error(result.stderr);
			return;
		}
		console.log(`✓ Preview ${name} destroyed`);
	},
});

const ls = defineCommand({
	meta: { name: "list", description: "List active preview environments" },
	args: {
		host: { type: "string", description: "SSH host" },
	},
	async run({ args }) {
		const config = await loadConfig();
		const host = args.host || config.host;
		const domain = config.domain;
		const runtime = config.runtime || "k3s";
		if (!host) {
			console.error("No host. Run: atlas infra setup");
			return;
		}
		if (!domain) {
			console.error("No domain configured.");
			return;
		}

		const cmd =
			runtime === "swarm"
				? `docker stack ls --format "{{.Name}}" | grep '^preview-' | while read s; do echo "$s $(docker stack services "$s" --format "{{.Replicas}}" | head -1)"; done`
				: `export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl get namespaces --no-headers | grep '^preview-' | awk '{print $1, $3}'`;

		const result = await ssh(host, cmd);
		if (!result.ok) {
			console.error(result.stderr);
			return;
		}

		const lines = result.stdout.trim().split("\n").filter(Boolean);
		if (lines.length === 0 || (lines.length === 1 && !lines[0]?.trim())) {
			console.log("No active previews");
			return;
		}

		console.log(pc.bold("\n🔮 Active Previews\n"));
		for (const line of lines) {
			const [ns, info] = line.split(/\s+/);
			const name = (ns ?? "").replace("preview-", "");
			console.log(
				`  ${pc.green("●")} ${name.padEnd(25)} ${pc.dim(info ?? "")} → https://${name}.preview.${domain}`,
			);
		}
		console.log();
	},
});

export default defineCommand({
	meta: { name: "preview", description: "Ephemeral preview environments" },
	subCommands: { start, stop, list: ls },
});
