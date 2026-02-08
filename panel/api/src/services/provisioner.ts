import { $ } from "bun"
import { eq } from "drizzle-orm"
import { db } from "../db"
import { servers, auditLog } from "../db/schema"

async function ssh(host: string, command: string): Promise<{ ok: boolean; stdout: string; stderr: string }> {
	try {
		const result = await $`ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 ${host} ${`bash -s <<'ATLAS_EOF'\n${command}\nATLAS_EOF`}`.quiet()
		return { ok: result.exitCode === 0, stdout: result.stdout.toString(), stderr: result.stderr.toString() }
	} catch (e: unknown) {
		const err = e as { stdout?: Buffer; stderr?: Buffer }
		return { ok: false, stdout: err.stdout?.toString() ?? "", stderr: err.stderr?.toString() ?? String(e) }
	}
}

interface ProvisionOptions {
	serverId: number
	host: string
	domain: string
	orgId: number
	skipMonitoring?: boolean
	skipArgocd?: boolean
}

export async function provisionServer(opts: ProvisionOptions): Promise<void> {
	const { serverId, host, domain, orgId } = opts

	const log = (msg: string) => console.log(`[provision:${serverId}] ${msg}`)

	try {
		// Test SSH
		log("Testing SSH...")
		const test = await ssh(host, "echo ok")
		if (!test.ok) throw new Error(`SSH failed: ${test.stderr}`)

		// Get server info
		const info = await ssh(host, "echo $(nproc) vCPU / $(free -h | awk '/Mem/{print $2}') RAM / $(df -h / | awk 'NR==2{print $4}') free")
		log(info.stdout.trim())

		// Get IP
		const ipResult = await ssh(host, "curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}'")
		const ip = ipResult.stdout.trim()

		await db.update(servers).set({ ip, status: "provisioning" }).where(eq(servers.id, serverId))

		const phases = [
			{ name: "System", script: phase01System() },
			{ name: "K3s + Helm", script: phase02K3s() },
			{ name: "Traefik + cert-manager", script: phase03Ingress(domain) },
		]

		if (!opts.skipMonitoring) {
			phases.push(
				{ name: "Prometheus + Grafana", script: phase04aPrometheus(domain) },
				{ name: "Loki", script: phase04bLoki() },
				{ name: "Alloy", script: phase04cAlloy() },
			)
		}

		if (!opts.skipArgocd) {
			phases.push({ name: "ArgoCD", script: phase05ArgoCD(domain) })
		}

		for (const phase of phases) {
			log(`${phase.name}...`)
			const result = await ssh(host, phase.script)
			if (!result.ok) {
				throw new Error(`${phase.name} failed: ${result.stderr || result.stdout}`)
			}
			log(`${phase.name} ✓`)
		}

		// Get kubeconfig
		const kcResult = await ssh(host, "cat /etc/rancher/k3s/k3s.yaml")
		const kubeconfig = kcResult.stdout.replace(/127\.0\.0\.1/g, ip)

		await db.update(servers).set({
			status: "online",
			ip,
			kubeconfigEnc: kubeconfig,
			meta: { provisionedAt: new Date().toISOString(), info: info.stdout.trim() },
		}).where(eq(servers.id, serverId))

		await db.insert(auditLog).values({
			orgId,
			action: "server.provisioned",
			resourceType: "server",
			resourceId: serverId,
			meta: { ip, info: info.stdout.trim() },
		})

		log("Server online ✓")
	} catch (e) {
		const error = e instanceof Error ? e.message : String(e)
		log(`FAILED: ${error}`)
		await db.update(servers).set({ status: "error", meta: { error } }).where(eq(servers.id, serverId))
	}
}

// ── Phase scripts (same as atlas infra setup) ──

function phase01System() {
	return `set -euo pipefail
if [ ! -f /swapfile ]; then
  fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile > /dev/null && swapon /swapfile
  grep -q '/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi
apt-get update -qq > /dev/null 2>&1
apt-get install -y -qq curl wget git jq open-iscsi nfs-common > /dev/null 2>&1
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
helm upgrade --install traefik traefik/traefik --namespace kube-system \
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
helm upgrade --install prometheus prometheus-community/kube-prometheus-stack --namespace monitoring \
  --set grafana.persistence.enabled=true --set grafana.persistence.size=5Gi \
  --set grafana.ingress.enabled=true --set "grafana.ingress.hosts[0]=grafana.${domain}" \
  --set "grafana.ingress.tls[0].secretName=grafana-tls" --set "grafana.ingress.tls[0].hosts[0]=grafana.${domain}" \
  --set 'grafana.ingress.annotations.cert-manager\\.io/cluster-issuer=letsencrypt-prod' \
  --set prometheus.prometheusSpec.retention=15d \
  --set prometheus.prometheusSpec.serviceMonitorSelectorNilUsesHelmValues=false \
  --set kubeApiServer.enabled=false --set kubeControllerManager.enabled=false \
  --set kubeProxy.enabled=false --set kubeScheduler.enabled=false --set kubeEtcd.enabled=false \
  --wait --timeout 5m
echo "ok"`
}

function phase04bLoki() {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm upgrade --install loki grafana/loki --namespace monitoring \
  --set deploymentMode=SingleBinary --set 'loki.commonConfig.replication_factor=1' \
  --set 'loki.auth_enabled=false' \
  --set 'loki.schemaConfig.configs[0].from=2024-04-01' --set 'loki.schemaConfig.configs[0].store=tsdb' \
  --set 'loki.schemaConfig.configs[0].object_store=s3' --set 'loki.schemaConfig.configs[0].schema=v13' \
  --set 'loki.schemaConfig.configs[0].index.prefix=loki_index_' --set 'loki.schemaConfig.configs[0].index.period=24h' \
  --set 'singleBinary.replicas=1' --set 'minio.enabled=true' \
  --set 'backend.replicas=0' --set 'read.replicas=0' --set 'write.replicas=0' \
  --set 'chunksCache.enabled=false' --set 'resultsCache.enabled=false' \
  --wait --timeout 5m
echo "ok"`
}

function phase04cAlloy() {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm upgrade --install alloy grafana/alloy --namespace monitoring --set 'controller.type=daemonset' --wait --timeout 3m
echo "ok"`
}

function phase05ArgoCD(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace argocd --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1
helm repo add argo https://argoproj.github.io/argo-helm > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install argocd argo/argo-cd --namespace argocd \
  --set server.ingress.enabled=true --set "server.ingress.hosts[0]=argocd.${domain}" \
  --set "server.ingress.tls[0].secretName=argocd-tls" --set "server.ingress.tls[0].hosts[0]=argocd.${domain}" \
  --set 'server.ingress.annotations.cert-manager\.io/cluster-issuer=letsencrypt-prod' \
  --set configs.params.server\\.insecure=true \
  --wait --timeout 5m > /dev/null 2>&1
echo "ok"`
}
