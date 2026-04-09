import type { ProvisionPhase } from "./index";
import { phase01System } from "./k3s";

export function phaseSwarmInstall() {
	return `set -euo pipefail
if ! command -v docker &> /dev/null; then
  curl -fsSL https://get.docker.com | sh > /dev/null 2>&1
  systemctl enable docker
  systemctl start docker
fi
if ! docker info --format '{{.Swarm.LocalNodeState}}' | grep -q active; then
  docker swarm init --advertise-addr $(hostname -I | awk '{print $1}') > /dev/null 2>&1
fi
echo "ok"`;
}

export function phaseSwarmNetwork() {
	return `set -euo pipefail
docker network create --driver overlay --attachable atlas-proxy 2>/dev/null || true
echo "ok"`;
}

export function phaseSwarmTraefik(domain: string) {
	return `set -euo pipefail
mkdir -p /opt/atlas/traefik
touch /opt/atlas/traefik/acme.json
chmod 600 /opt/atlas/traefik/acme.json

cat > /opt/atlas/traefik/docker-compose.yml <<'COMPOSE'
services:
  traefik:
    image: traefik:v3.4
    command:
      - "--providers.swarm=true"
      - "--providers.swarm.exposedByDefault=false"
      - "--providers.swarm.network=atlas-proxy"
      - "--entrypoints.web.address=:80"
      - "--entrypoints.websecure.address=:443"
      - "--entrypoints.web.http.redirections.entryPoint.to=websecure"
      - "--entrypoints.web.http.redirections.entryPoint.scheme=https"
      - "--entrypoints.web.http.redirections.entryPoint.permanent=true"
      - "--entrypoints.websecure.http.tls=true"
      - "--certificatesresolvers.le.acme.email=admin@${domain}"
      - "--certificatesresolvers.le.acme.storage=/letsencrypt/acme.json"
      - "--certificatesresolvers.le.acme.httpchallenge.entrypoint=web"
      - "--api.dashboard=false"
      - "--log.level=WARN"
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - /opt/atlas/traefik/acme.json:/letsencrypt/acme.json
    networks:
      - atlas-proxy
    deploy:
      placement:
        constraints:
          - node.role == manager
      restart_policy:
        condition: any

networks:
  atlas-proxy:
    external: true
COMPOSE

cd /opt/atlas/traefik && docker stack deploy -c docker-compose.yml traefik
sleep 5
echo "ok"`;
}

export function phaseSwarmMonitoring(domain: string) {
	return `set -euo pipefail
mkdir -p /opt/atlas/monitoring/prometheus
mkdir -p /opt/atlas/monitoring/loki

GRAFANA_PASS=$(openssl rand -base64 16 | tr -d '=/+' | head -c 20)
echo "\${GRAFANA_PASS}" > /opt/atlas/monitoring/.grafana-pass

cat > /opt/atlas/monitoring/prometheus/prometheus.yml <<'PROMCFG'
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: prometheus
    static_configs:
      - targets: ["localhost:9090"]
  - job_name: node-exporter
    static_configs:
      - targets: ["node-exporter:9100"]
  - job_name: cadvisor
    static_configs:
      - targets: ["cadvisor:8080"]
PROMCFG

cat > /opt/atlas/monitoring/docker-compose.yml <<COMPOSE
services:
  prometheus:
    image: prom/prometheus:latest
    volumes:
      - /opt/atlas/monitoring/prometheus/prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    command: ["--config.file=/etc/prometheus/prometheus.yml", "--storage.tsdb.retention.time=15d"]
    networks:
      - monitoring
      - atlas-proxy
    deploy:
      placement:
        constraints: [node.role == manager]
      labels:
        - "traefik.enable=true"
        - "traefik.http.routers.prometheus.rule=Host(\\\`prometheus.${domain}\\\`)"
        - "traefik.http.routers.prometheus.tls.certresolver=le"
        - "traefik.http.services.prometheus.loadbalancer.server.port=9090"

  grafana:
    image: grafana/grafana:latest
    volumes:
      - grafana-data:/var/lib/grafana
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=\${GRAFANA_PASS}
      - GF_SERVER_ROOT_URL=https://grafana.${domain}
    networks:
      - monitoring
      - atlas-proxy
    deploy:
      labels:
        - "traefik.enable=true"
        - "traefik.http.routers.grafana.rule=Host(\\\`grafana.${domain}\\\`)"
        - "traefik.http.routers.grafana.tls.certresolver=le"
        - "traefik.http.services.grafana.loadbalancer.server.port=3000"

  loki:
    image: grafana/loki:latest
    command: ["-config.file=/etc/loki/local-config.yaml"]
    volumes:
      - loki-data:/loki
    networks:
      - monitoring

  promtail:
    image: grafana/promtail:latest
    volumes:
      - /var/log:/var/log:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
    command: ["-config.file=/etc/promtail/config.yml"]
    networks:
      - monitoring
    deploy:
      mode: global

  node-exporter:
    image: prom/node-exporter:latest
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /:/rootfs:ro
    command: ["--path.procfs=/host/proc", "--path.sysfs=/host/sys", "--path.rootfs=/rootfs"]
    networks:
      - monitoring
    deploy:
      mode: global

  cadvisor:
    image: gcr.io/cadvisor/cadvisor:latest
    volumes:
      - /:/rootfs:ro
      - /var/run:/var/run:ro
      - /sys:/sys:ro
      - /var/lib/docker/:/var/lib/docker:ro
    networks:
      - monitoring
    deploy:
      mode: global

volumes:
  prometheus-data:
  grafana-data:
  loki-data:

networks:
  monitoring:
    driver: overlay
  atlas-proxy:
    external: true
COMPOSE

cd /opt/atlas/monitoring && docker stack deploy -c docker-compose.yml monitoring
echo "ok"`;
}

export function phaseSwarmApp(domain: string) {
	const ns = domain.split(".")[0];
	return `set -euo pipefail
mkdir -p /opt/atlas/apps/${ns}
echo "ok"`;
}

export interface SwarmProvisionOptions {
	domain: string;
	skipMonitoring?: boolean;
	tunnel?: { cfToken: string; cfAccount: string };
}

export function getSwarmPhases(opts: SwarmProvisionOptions): ProvisionPhase[] {
	const phases: ProvisionPhase[] = [
		{ name: "System preparation", script: phase01System() },
		{ name: "Docker + Swarm init", script: phaseSwarmInstall() },
		{ name: "Overlay network", script: phaseSwarmNetwork() },
		{ name: "Traefik (HTTPS)", script: phaseSwarmTraefik(opts.domain) },
	];

	if (!opts.skipMonitoring) {
		phases.push({
			name: "Monitoring (Prometheus + Grafana + Loki)",
			script: phaseSwarmMonitoring(opts.domain),
		});
	}

	phases.push({ name: "Application stack", script: phaseSwarmApp(opts.domain) });

	return phases;
}
