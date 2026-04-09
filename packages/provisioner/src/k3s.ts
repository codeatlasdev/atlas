import type { ProvisionPhase } from "./index";

export function phase01System() {
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
echo "ok"`;
}

export function phaseK3sInstall() {
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
echo "ok"`;
}

export function phaseK3sIngress(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm repo add traefik https://traefik.github.io/charts > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install traefik traefik/traefik --namespace kube-system \\
  --set 'ports.web.http.redirections.entryPoint.to=websecure' \\
  --set 'ports.web.http.redirections.entryPoint.scheme=https' \\
  --set 'ports.web.http.redirections.entryPoint.permanent=true' \\
  --set ingressRoute.dashboard.enabled=false \\
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
echo "ok"`;
}

export function phaseK3sPrometheus(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace monitoring --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts > /dev/null 2>&1
helm repo add grafana https://grafana.github.io/helm-charts > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install prometheus prometheus-community/kube-prometheus-stack --namespace monitoring \\
  --set grafana.persistence.enabled=true --set grafana.persistence.size=5Gi \\
  --set grafana.ingress.enabled=true --set "grafana.ingress.hosts[0]=grafana.${domain}" \\
  --set "grafana.ingress.tls[0].secretName=grafana-tls" --set "grafana.ingress.tls[0].hosts[0]=grafana.${domain}" \\
  --set 'grafana.ingress.annotations.cert-manager\\.io/cluster-issuer=letsencrypt-prod' \\
  --set prometheus.prometheusSpec.retention=15d \\
  --set prometheus.prometheusSpec.serviceMonitorSelectorNilUsesHelmValues=false \\
  --set prometheus.prometheusSpec.podMonitorSelectorNilUsesHelmValues=false \\
  --set prometheus.prometheusSpec.ruleSelectorNilUsesHelmValues=false \\
  --set kubeApiServer.enabled=false --set kubeControllerManager.enabled=false \\
  --set kubeProxy.enabled=false --set kubeScheduler.enabled=false --set kubeEtcd.enabled=false \\
  --set 'defaultRules.rules.etcd=false' --set 'defaultRules.rules.kubeProxy=false' \\
  --set 'defaultRules.rules.kubeSchedulerAlerting=false' --set 'defaultRules.rules.kubeSchedulerRecording=false' \\
  --set 'defaultRules.rules.kubeControllerManager=false' \\
  --wait --timeout 5m
echo "ok"`;
}

export function phaseK3sLoki() {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm upgrade --install loki grafana/loki --namespace monitoring \\
  --set deploymentMode=SingleBinary --set 'loki.commonConfig.replication_factor=1' \\
  --set 'loki.auth_enabled=false' \\
  --set 'loki.schemaConfig.configs[0].from=2024-04-01' --set 'loki.schemaConfig.configs[0].store=tsdb' \\
  --set 'loki.schemaConfig.configs[0].object_store=s3' --set 'loki.schemaConfig.configs[0].schema=v13' \\
  --set 'loki.schemaConfig.configs[0].index.prefix=loki_index_' --set 'loki.schemaConfig.configs[0].index.period=24h' \\
  --set 'singleBinary.replicas=1' --set 'minio.enabled=true' \\
  --set 'backend.replicas=0' --set 'read.replicas=0' --set 'write.replicas=0' \\
  --set 'chunksCache.enabled=false' --set 'resultsCache.enabled=false' \\
  --wait --timeout 5m
echo "ok"`;
}

export function phaseK3sAlloy() {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm upgrade --install alloy grafana/alloy --namespace monitoring --set 'controller.type=daemonset' --wait --timeout 3m
echo "ok"`;
}

export function phaseK3sArgoCD(domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace argocd --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1
helm repo add argo https://argoproj.github.io/argo-helm > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install argocd argo/argo-cd --namespace argocd \\
  --set server.ingress.enabled=true --set "server.ingress.hosts[0]=argocd.${domain}" \\
  --set "server.ingress.tls[0].secretName=argocd-tls" --set "server.ingress.tls[0].hosts[0]=argocd.${domain}" \\
  --set 'server.ingress.annotations.cert-manager\\.io/cluster-issuer=letsencrypt-prod' \\
  --set configs.params.server\\.insecure=true \\
  --wait --timeout 5m > /dev/null 2>&1
echo "ok"`;
}

export function phaseK3sApp(domain: string) {
	const ns = domain.split(".")[0];
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
kubectl create namespace ${ns} --dry-run=client -o yaml | kubectl apply -f - > /dev/null 2>&1

if ! kubectl -n ${ns} get secret ${ns}-secrets &> /dev/null; then
  PG_PASS=$(openssl rand -base64 16 | tr -d '=/+' | head -c 20)
  AUTH_SECRET=$(openssl rand -base64 32)
  ENC_KEY=$(openssl rand -hex 32)
  kubectl -n ${ns} create secret generic ${ns}-secrets \\
    --from-literal=DATABASE_URL="postgresql://${ns}:\${PG_PASS}@postgres:5432/${ns}" \\
    --from-literal=POSTGRES_USER=${ns} \\
    --from-literal=POSTGRES_PASSWORD="\${PG_PASS}" \\
    --from-literal=BETTER_AUTH_SECRET="\${AUTH_SECRET}" \\
    --from-literal=BETTER_AUTH_URL="https://api.${domain}" \\
    --from-literal=BASE_URL="https://api.${domain}" \\
    --from-literal=WEB_URL="https://backoffice.${domain}" \\
    --from-literal=TRUSTED_ORIGINS="https://backoffice.${domain},https://api.${domain}" \\
    --from-literal=COOKIE_DOMAIN=".${domain}" \\
    --from-literal=REDIS_URL="redis://redis:6379" \\
    --from-literal=ENCRYPTION_KEY="\${ENC_KEY}"
fi
echo "ok"`;
}

export function phaseK3sTunnel(cfToken: string, cfAccount: string, domain: string) {
	return `set -euo pipefail
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
helm repo add strrl.dev https://helm.strrl.dev > /dev/null 2>&1
helm repo update > /dev/null 2>&1
helm upgrade --install cloudflare-tunnel-ingress-controller \\
  strrl.dev/cloudflare-tunnel-ingress-controller \\
  --namespace cloudflare-tunnel --create-namespace \\
  --set cloudflare.apiToken="${cfToken}" \\
  --set cloudflare.accountId="${cfAccount}" \\
  --set cloudflare.tunnelName="atlas-${domain}" \\
  --wait --timeout 3m > /dev/null 2>&1
echo "ok"`;
}

export interface K3sProvisionOptions {
	domain: string;
	skipMonitoring?: boolean;
	skipArgocd?: boolean;
	tunnel?: { cfToken: string; cfAccount: string };
}

export function getK3sPhases(opts: K3sProvisionOptions): ProvisionPhase[] {
	const phases: ProvisionPhase[] = [
		{ name: "System preparation", script: phase01System() },
		{ name: "K3s + Helm", script: phaseK3sInstall() },
		{ name: "Traefik + cert-manager", script: phaseK3sIngress(opts.domain) },
	];

	if (!opts.skipMonitoring) {
		phases.push(
			{ name: "Prometheus + Grafana", script: phaseK3sPrometheus(opts.domain) },
			{ name: "Loki", script: phaseK3sLoki() },
			{ name: "Alloy (log collector)", script: phaseK3sAlloy() },
		);
	}

	if (!opts.skipArgocd) {
		phases.push({ name: "ArgoCD", script: phaseK3sArgoCD(opts.domain) });
	}

	if (opts.tunnel) {
		phases.push({
			name: "Cloudflare Tunnel",
			script: phaseK3sTunnel(opts.tunnel.cfToken, opts.tunnel.cfAccount, opts.domain),
		});
	}

	phases.push({ name: "Application namespace", script: phaseK3sApp(opts.domain) });

	return phases;
}
