import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { ssh } from "../../lib/ssh"
import { loadConfig, saveConfig } from "../../lib/config"

export default defineCommand({
	meta: {
		name: "setup",
		description: "Setup a fresh server with K3s, Traefik, cert-manager, monitoring, and ArgoCD",
	},
	args: {
		host: {
			type: "string",
			description: "SSH host (e.g., root@1.2.3.4 or ssh alias)",
		},
		domain: {
			type: "string",
			description: "Base domain (e.g., myapp.com)",
		},
		"skip-monitoring": {
			type: "boolean",
			description: "Skip monitoring stack installation",
			default: false,
		},
		"skip-argocd": {
			type: "boolean",
			description: "Skip ArgoCD installation",
			default: false,
		},
		tunnel: {
			type: "boolean",
			description: "Use Cloudflare Tunnel (zero DNS config, zero open ports)",
			default: false,
		},
		"cf-token": {
			type: "string",
			description: "Cloudflare API token (for tunnel mode)",
		},
		"cf-account": {
			type: "string",
			description: "Cloudflare account ID (for tunnel mode)",
		},
		yes: {
			type: "boolean",
			alias: "y",
			description: "Skip all prompts (non-interactive mode)",
			default: false,
		},
	},
	async run({ args }) {
		const auto = args.yes

		if (!auto) p.intro(pc.bgCyan(pc.black(" atlas infra setup ")))

		const config = await loadConfig()

		// Resolve host
		const host =
			args.host ||
			(auto
				? config.host
				: await p.text({
						message: "SSH host (e.g., root@1.2.3.4 or ssh config alias)",
						placeholder: config.host || "root@1.2.3.4",
						defaultValue: config.host,
						validate: (v) => (!v ? "Host is required" : undefined),
					}))

		if (!host) { console.error("--host is required in non-interactive mode"); return }
		if (p.isCancel(host)) return p.cancel("Cancelled")

		// Resolve domain
		const domain =
			args.domain ||
			(auto
				? config.domain
				: await p.text({
						message: "Base domain",
						placeholder: config.domain || "myapp.com",
						defaultValue: config.domain,
						validate: (v) => (!v ? "Domain is required" : undefined),
					}))

		if (!domain) { console.error("--domain is required in non-interactive mode"); return }
		if (p.isCancel(domain)) return p.cancel("Cancelled")

		const log = auto
			? { start: (m: string) => console.log(`→ ${m}`), stop: (m: string) => console.log(`✓ ${m}`) }
			: p.spinner()

		// Test SSH connection
		log.start("Testing SSH connection...")

		try {
			const result = await ssh(host as string, "echo ok")
			if (!result.ok) throw new Error(result.stderr)
			log.stop("SSH connection OK")
		} catch (e) {
			log.stop("SSH connection failed")
			console.error(`Cannot connect to ${host}: ${e}`)
			return
		}

		// Get server info
		log.start("Checking server...")
		const info = await ssh(
			host as string,
			"echo $(grep PRETTY_NAME /etc/os-release | cut -d'\"' -f2) '|' $(free -h | awk '/Mem/{print $2}') RAM '|' $(nproc) vCPU",
		)
		log.stop(info.stdout.trim())

		// Confirm
		if (!auto) {
			const proceed = await p.confirm({
				message: `Setup ${pc.bold(host as string)} with domain ${pc.bold(domain as string)}?`,
			})
			if (p.isCancel(proceed) || !proceed) return p.cancel("Cancelled")
		}

		// Save config
		await saveConfig({ host: host as string, domain: domain as string })

		// Run phases
		const phases = [
			{ name: "System preparation", script: phase01System() },
			{ name: "K3s + Helm", script: phase02K3s() },
			{
				name: "Traefik + cert-manager",
				script: phase03Ingress(domain as string),
			},
		]

		if (!args["skip-monitoring"]) {
			phases.push(
				{ name: "Prometheus + Grafana", script: phase04aPrometheus(domain as string) },
				{ name: "Loki", script: phase04bLoki() },
				{ name: "Alloy (log collector)", script: phase04cAlloy() },
			)
		}

		if (!args["skip-argocd"]) {
			phases.push({ name: "ArgoCD", script: phase05ArgoCD(domain as string) })
		}

		// Cloudflare Tunnel (replaces manual DNS + cert-manager for app traffic)
		if (args.tunnel) {
			const cfToken = args["cf-token"] || process.env.CLOUDFLARE_API_TOKEN
			const cfAccount = args["cf-account"] || process.env.CLOUDFLARE_ACCOUNT_ID
			if (!cfToken || !cfAccount) {
				log.stop("Cloudflare Tunnel requires --cf-token and --cf-account (or CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID env vars)")
				return
			}
			phases.push({
				name: "Cloudflare Tunnel Ingress Controller",
				script: phase07Tunnel(cfToken as string, cfAccount as string, domain as string),
			})
		}

		phases.push({ name: "Application namespace", script: phase06App(domain as string) })

		for (const phase of phases) {
			log.start(phase.name)
			const result = await ssh(host as string, phase.script)
			if (!result.ok) {
				log.stop(`${phase.name} — FAILED`)
				console.error(result.stderr || result.stdout)
				return
			}
			log.stop(`${phase.name} — done`)
		}

		// Get credentials
		const creds = await ssh(
			host as string,
			'export KUBECONFIG=/etc/rancher/k3s/k3s.yaml; echo "ARGOCD_PASS=$(kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath=\'{.data.password}\' 2>/dev/null | base64 -d 2>/dev/null || echo N/A)"',
		)

		const serverIp = await ssh(host as string, "curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'")

		p.note(
			[
				`${pc.bold("DNS")} — Point to ${pc.cyan(serverIp.stdout.trim())}:`,
				`  api.${domain}`,
				`  backoffice.${domain}`,
				`  finances.${domain}`,
				`  bi.${domain}`,
				!args["skip-monitoring"] ? `  grafana.${domain}` : "",
				!args["skip-argocd"] ? `  argocd.${domain}` : "",
				`  *.preview.${domain}`,
				"",
				creds.stdout.trim(),
			]
				.filter(Boolean)
				.join("\n"),
			"Setup complete",
		)

		if (!auto) p.outro(pc.green("Server ready! Push to main to deploy."))
		else console.log("✓ Server ready! Push to main to deploy.")
	},
})

// ── Phase scripts ────────────────────────────

function phase01System() {
	return `set -euo pipefail
if [ ! -f /swapfile ]; then
  fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile > /dev/null && swapon /swapfile
  grep -q '/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  sysctl -w vm.swappiness=10 > /dev/null
  grep -q 'vm.swappiness' /etc/sysctl.conf || echo 'vm.swappiness=10' >> /etc/sysctl.conf
fi
apt-get update -qq > /dev/null 2>&1
apt-get install -y -qq curl wget git jq open-iscsi nfs-common > /dev/null 2>&1
timedatectl set-timezone America/Sao_Paulo 2>/dev/null || true
echo "ok"`
}

function phase02K3s() {
	return `set -euo pipefail
if ! command -v k3s &> /dev/null; then
  curl -sfL https://get.k3s.io | sh -s - --write-kubeconfig-mode 644 --disable traefik
  sleep 10
fi
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl wait --for=condition=Ready node --all --timeout=120s > /dev/null 2>&1
if ! command -v helm &> /dev/null; then
  curl -fsSL https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash > /dev/null 2>&1
fi
echo "ok"`
}

function phase03Ingress(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm repo add traefik https://traefik.github.io/charts > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install traefik traefik/traefik \
  --namespace kube-system \
  --set 'ports.web.http.redirections.entryPoint.to=websecure' \
  --set 'ports.web.http.redirections.entryPoint.scheme=https' \
  --set 'ports.web.http.redirections.entryPoint.permanent=true' \
  --set ingressRoute.dashboard.enabled=false \
  --wait --timeout 3m > /dev/null 2>&1
if ! kubectl get namespace cert-manager &> /dev/null; then
  kubectl apply -f https://github.com/cert-manager/cert-manager/releases/latest/download/cert-manager.yaml > /dev/null 2>&1
  sleep 15
fi
kubectl -n cert-manager wait --for=condition=Available deployment --all --timeout=120s > /dev/null 2>&1
cat <<EOF | kubectl apply -f - > /dev/null 2>&1
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: admin@${domain}
    privateKeySecretRef:
      name: letsencrypt-prod
    solvers:
      - http01:
          ingress:
            class: traefik
EOF
echo "ok"`
}

function phase04aPrometheus(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace monitoring --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts > /dev/null 2>&1
helm repo add grafana https://grafana.github.io/helm-charts > /dev/null 2>&1
helm repo update > /dev/null 2>&1

helm upgrade --install prometheus prometheus-community/kube-prometheus-stack \
  --namespace monitoring \
  --set grafana.persistence.enabled=true \
  --set grafana.persistence.size=5Gi \
  --set grafana.ingress.enabled=true \
  --set "grafana.ingress.hosts[0]=grafana.${domain}" \
  --set "grafana.ingress.tls[0].secretName=grafana-tls" \
  --set "grafana.ingress.tls[0].hosts[0]=grafana.${domain}" \
  --set 'grafana.ingress.annotations.cert-manager\\.io/cluster-issuer=letsencrypt-prod' \
  --set prometheus.prometheusSpec.retention=15d \
  --set prometheus.prometheusSpec.serviceMonitorSelectorNilUsesHelmValues=false \
  --set prometheus.prometheusSpec.podMonitorSelectorNilUsesHelmValues=false \
  --set prometheus.prometheusSpec.ruleSelectorNilUsesHelmValues=false \
  --set kubeApiServer.enabled=false \
  --set kubeControllerManager.enabled=false \
  --set kubeProxy.enabled=false \
  --set kubeScheduler.enabled=false \
  --set kubeEtcd.enabled=false \
  --set 'defaultRules.rules.etcd=false' \
  --set 'defaultRules.rules.kubeProxy=false' \
  --set 'defaultRules.rules.kubeSchedulerAlerting=false' \
  --set 'defaultRules.rules.kubeSchedulerRecording=false' \
  --set 'defaultRules.rules.kubeControllerManager=false' \
  --wait --timeout 5m
echo "ok"`
}

function phase04bLoki() {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm upgrade --install loki grafana/loki \
  --namespace monitoring \
  --set deploymentMode=SingleBinary \
  --set 'loki.commonConfig.replication_factor=1' \
  --set 'loki.auth_enabled=false' \
  --set 'loki.schemaConfig.configs[0].from=2024-04-01' \
  --set 'loki.schemaConfig.configs[0].store=tsdb' \
  --set 'loki.schemaConfig.configs[0].object_store=s3' \
  --set 'loki.schemaConfig.configs[0].schema=v13' \
  --set 'loki.schemaConfig.configs[0].index.prefix=loki_index_' \
  --set 'loki.schemaConfig.configs[0].index.period=24h' \
  --set 'singleBinary.replicas=1' \
  --set 'minio.enabled=true' \
  --set 'backend.replicas=0' \
  --set 'read.replicas=0' \
  --set 'write.replicas=0' \
  --set 'ingester.replicas=0' \
  --set 'querier.replicas=0' \
  --set 'queryFrontend.replicas=0' \
  --set 'queryScheduler.replicas=0' \
  --set 'distributor.replicas=0' \
  --set 'compactor.replicas=0' \
  --set 'indexGateway.replicas=0' \
  --set 'bloomCompactor.replicas=0' \
  --set 'bloomGateway.replicas=0' \
  --set 'chunksCache.enabled=false' \
  --set 'resultsCache.enabled=false' \
  --wait --timeout 5m
echo "ok"`
}

function phase04cAlloy() {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm upgrade --install alloy grafana/alloy \
  --namespace monitoring \
  --set 'controller.type=daemonset' \
  --wait --timeout 3m
echo "ok"`
}

function phase05ArgoCD(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace argocd --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1
helm repo add argo https://argoproj.github.io/argo-helm > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install argocd argo/argo-cd \
  --namespace argocd \
  --set server.ingress.enabled=true \
  --set "server.ingress.hosts[0]=argocd.${domain}" \
  --set "server.ingress.tls[0].secretName=argocd-tls" \
  --set "server.ingress.tls[0].hosts[0]=argocd.${domain}" \
  --set 'server.ingress.annotations.cert-manager\.io/cluster-issuer=letsencrypt-prod' \
  --set configs.params.server\\.insecure=true \
  --wait --timeout 5m > /dev/null 2>&1
echo "ok"`
}

function phase06App(domain: string) {
	const ns = domain.split(".")[0]
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace ${ns} --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1

# Generate secrets if they don't exist
if ! kubectl -n ${ns} get secret ${ns}-secrets &> /dev/null; then
  PG_PASS=$(openssl rand -base64 16 | tr -d '=/+' | head -c 20)
  AUTH_SECRET=$(openssl rand -base64 32)
  ENC_KEY=$(openssl rand -hex 32)
  kubectl -n ${ns} create secret generic ${ns}-secrets \
    --from-literal=DATABASE_URL="postgresql://${ns}:\${PG_PASS}@postgres:5432/${ns}" \
    --from-literal=POSTGRES_USER=${ns} \
    --from-literal=POSTGRES_PASSWORD="\${PG_PASS}" \
    --from-literal=BETTER_AUTH_SECRET="\${AUTH_SECRET}" \
    --from-literal=BETTER_AUTH_URL="https://api.${domain}" \
    --from-literal=BASE_URL="https://api.${domain}" \
    --from-literal=WEB_URL="https://backoffice.${domain}" \
    --from-literal=TRUSTED_ORIGINS="https://backoffice.${domain},https://finances.${domain},https://bi.${domain},https://api.${domain}" \
    --from-literal=COOKIE_DOMAIN=".${domain}" \
    --from-literal=REDIS_URL="redis://redis:6379" \
    --from-literal=ENCRYPTION_KEY="\${ENC_KEY}"
fi
echo "ok"`
}

function phase07Tunnel(cfToken: string, cfAccount: string, domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

# Install Cloudflare Tunnel Ingress Controller
helm repo add strrl.dev https://helm.strrl.dev > /dev/null 2>&1
helm repo update > /dev/null 2>&1

helm upgrade --install cloudflare-tunnel-ingress-controller \
  strrl.dev/cloudflare-tunnel-ingress-controller \
  --namespace cloudflare-tunnel --create-namespace \
  --set cloudflare.apiToken="${cfToken}" \
  --set cloudflare.accountId="${cfAccount}" \
  --set cloudflare.tunnelName="atlas-${domain}" \
  --wait --timeout 3m > /dev/null 2>&1

echo "ok"`
}
