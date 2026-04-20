import type { ProvisionPhase } from "@atlas/api/types";

export function phaseCheckKVM(): string {
	return `set -euo pipefail
if [ ! -e /dev/kvm ]; then
  echo "FATAL: /dev/kvm not found. Firecracker requires bare metal with KVM." >&2
  exit 1
fi
if ! grep -qE '(vmx|svm)' /proc/cpuinfo; then
  echo "FATAL: CPU does not support hardware virtualization (VT-x/AMD-V)" >&2
  exit 1
fi
echo "ok"`;
}

export function phaseInstallFirecracker(): string {
	return `set -euo pipefail
FC_VERSION="v1.15.1"
ARCH=$(uname -m)

if [ -f /usr/local/bin/firecracker ]; then
  CURRENT=$(/usr/local/bin/firecracker --version 2>/dev/null | head -1 | awk '{print $2}' || echo "")
  if [ "$CURRENT" = "$FC_VERSION" ]; then
    echo "ok"
    exit 0
  fi
fi

mkdir -p /opt/atlas/firecracker
cd /tmp
curl -sL "https://github.com/firecracker-microvm/firecracker/releases/download/\${FC_VERSION}/firecracker-\${FC_VERSION}-\${ARCH}.tgz" -o fc.tgz
tar xzf fc.tgz
mv release-*/firecracker-* /usr/local/bin/firecracker
chmod +x /usr/local/bin/firecracker
rm -rf release-* fc.tgz

# Kernel image
if [ ! -f /opt/atlas/firecracker/vmlinux.bin ]; then
  curl -sL "https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/\${ARCH}/kernels/vmlinux.bin" \
    -o /opt/atlas/firecracker/vmlinux.bin
fi

echo "ok"`;
}

export function phaseBuildBaseRootfs(): string {
	return `set -euo pipefail
ROOTFS_DIR="/opt/atlas/firecracker/rootfs"
BASE_IMG="$ROOTFS_DIR/base.ext4"

if [ -f "$BASE_IMG" ]; then
  echo "ok"
  exit 0
fi

mkdir -p "$ROOTFS_DIR"

# Install Docker if not present (needed for rootfs build only)
if ! command -v docker &> /dev/null; then
  curl -fsSL https://get.docker.com | sh > /dev/null 2>&1
fi

# Build minimal rootfs via Docker
docker rm -f atlas-rootfs 2>/dev/null || true
docker pull debian:bookworm-slim > /dev/null 2>&1
docker run --name atlas-rootfs debian:bookworm-slim bash -c "
  apt-get update -qq > /dev/null 2>&1
  apt-get install -y -qq --no-install-recommends \
    ca-certificates curl libgcc-s1 libc6 libssl3 > /dev/null 2>&1
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash - > /dev/null 2>&1
  apt-get install -y -qq nodejs > /dev/null 2>&1
  apt-get clean > /dev/null 2>&1
  rm -rf /var/lib/apt/lists/*
"

# Export filesystem
mkdir -p /tmp/atlas-rootfs
docker export atlas-rootfs | tar xf - -C /tmp/atlas-rootfs
docker rm atlas-rootfs > /dev/null 2>&1

# Create init script
cat > /tmp/atlas-rootfs/sbin/atlas-init <<'INIT'
#!/bin/sh
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev

# Load env vars
if [ -f /etc/atlas/env ]; then
  set -a
  . /etc/atlas/env
  set +a
fi

# Start app
if [ -f /app/start.sh ]; then
  exec /app/start.sh
elif [ -f /app/server.js ]; then
  exec node /app/server.js
elif [ -f /app/index.js ]; then
  exec node /app/index.js
else
  echo "No entrypoint found in /app" >&2
  exec /bin/sh
fi
INIT
chmod +x /tmp/atlas-rootfs/sbin/atlas-init
ln -sf /sbin/atlas-init /tmp/atlas-rootfs/sbin/init

mkdir -p /tmp/atlas-rootfs/app
mkdir -p /tmp/atlas-rootfs/etc/atlas

# Create ext4 image
mke2fs -t ext4 -d /tmp/atlas-rootfs "$BASE_IMG" 512M > /dev/null 2>&1
rm -rf /tmp/atlas-rootfs

echo "ok"`;
}

export function phaseInstallVMM(): string {
	return `set -euo pipefail
mkdir -p /opt/atlas/firecracker/vms
mkdir -p /var/log/atlas-vmm

# Create systemd service for atlas-vmm
cat > /etc/systemd/system/atlas-vmm.service <<'SERVICE'
[Unit]
Description=Atlas VMM - Firecracker VM Manager
After=network.target

[Service]
Type=simple
ExecStart=/opt/atlas/firecracker/atlas-vmm
Restart=always
RestartSec=5
Environment=VMM_SOCKET=/var/run/atlas-vmm.sock
Environment=FC_BIN=/usr/local/bin/firecracker
Environment=KERNEL=/opt/atlas/firecracker/vmlinux.bin
Environment=BASE_ROOTFS=/opt/atlas/firecracker/rootfs/base.ext4
Environment=VM_DIR=/opt/atlas/firecracker/vms
WorkingDirectory=/opt/atlas/firecracker

[Install]
WantedBy=multi-user.target
SERVICE

systemctl daemon-reload
echo "ok"`;
}

export function phaseFirecrackerTraefik(domain: string): string {
	return `set -euo pipefail
# Install Traefik as static binary (no Docker needed for Firecracker runtime)
if ! command -v traefik &> /dev/null; then
  ARCH=$(uname -m)
  case $ARCH in
    x86_64) TRAEFIK_ARCH="amd64" ;;
    aarch64) TRAEFIK_ARCH="arm64" ;;
  esac
  curl -sL "https://github.com/traefik/traefik/releases/latest/download/traefik_v3.4.0_linux_\${TRAEFIK_ARCH}.tar.gz" | tar xz -C /usr/local/bin traefik
  chmod +x /usr/local/bin/traefik
fi

mkdir -p /opt/atlas/traefik
touch /opt/atlas/traefik/acme.json
chmod 600 /opt/atlas/traefik/acme.json

cat > /opt/atlas/traefik/traefik.yml <<EOF
entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
          permanent: true
  websecure:
    address: ":443"

providers:
  file:
    directory: /opt/atlas/traefik/conf.d
    watch: true

certificatesResolvers:
  le:
    acme:
      email: admin@${domain}
      storage: /opt/atlas/traefik/acme.json
      httpChallenge:
        entryPoint: web

api:
  dashboard: false

log:
  level: WARN
EOF

mkdir -p /opt/atlas/traefik/conf.d

cat > /etc/systemd/system/traefik.service <<'SERVICE'
[Unit]
Description=Traefik Proxy
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/traefik --configFile=/opt/atlas/traefik/traefik.yml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
SERVICE

systemctl daemon-reload
systemctl enable traefik
systemctl start traefik
echo "ok"`;
}

export function phaseFirecrackerMonitoring(domain: string): string {
	return `set -euo pipefail
# Lightweight monitoring without Docker — just node-exporter + prometheus as binaries
ARCH=$(uname -m)
case $ARCH in
  x86_64) PROM_ARCH="amd64" ;;
  aarch64) PROM_ARCH="arm64" ;;
esac

# Node exporter
if ! command -v node_exporter &> /dev/null; then
  cd /tmp
  curl -sL "https://github.com/prometheus/node_exporter/releases/latest/download/node_exporter-1.9.1.linux-\${PROM_ARCH}.tar.gz" | tar xz
  mv node_exporter-*/node_exporter /usr/local/bin/
  rm -rf node_exporter-*
fi

cat > /etc/systemd/system/node-exporter.service <<'SERVICE'
[Unit]
Description=Node Exporter
After=network.target
[Service]
Type=simple
ExecStart=/usr/local/bin/node_exporter
Restart=always
[Install]
WantedBy=multi-user.target
SERVICE

systemctl daemon-reload
systemctl enable node-exporter
systemctl start node-exporter
echo "ok"`;
}

export interface FirecrackerProvisionOptions {
	domain: string;
	skipMonitoring?: boolean;
}

export function getFirecrackerPhases(opts: FirecrackerProvisionOptions): ProvisionPhase[] {
	// Reuse system prep from k3s provisioner
	const phases: ProvisionPhase[] = [
		{
			name: "System preparation",
			script: `set -euo pipefail
if [ ! -f /swapfile ]; then
  fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile > /dev/null && swapon /swapfile
  grep -q '/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  sysctl -w vm.swappiness=10 > /dev/null
  grep -q 'vm.swappiness' /etc/sysctl.conf || echo 'vm.swappiness=10' >> /etc/sysctl.conf
fi
apt-get update -qq > /dev/null 2>&1
apt-get install -y -qq curl wget git jq e2fsprogs > /dev/null 2>&1
timedatectl set-timezone America/Sao_Paulo 2>/dev/null || true
echo "ok"`,
		},
		{ name: "Verify KVM support", script: phaseCheckKVM() },
		{ name: "Install Firecracker", script: phaseInstallFirecracker() },
		{ name: "Build base rootfs", script: phaseBuildBaseRootfs() },
		{ name: "Install atlas-vmm service", script: phaseInstallVMM() },
		{ name: "Traefik (HTTPS)", script: phaseFirecrackerTraefik(opts.domain) },
	];

	if (!opts.skipMonitoring) {
		phases.push({ name: "Monitoring", script: phaseFirecrackerMonitoring(opts.domain) });
	}

	return phases;
}
