# The "server" of scripts/crew-e2e.sh: a plain Ubuntu 24.04 with systemd as PID 1 and sshd, the way a fresh
# VPS looks before `atlas crew install` (sudo, curl, tar, git, nftables, an admin user with passwordless sudo).
# Nothing of Atlas is in it; the install puts everything there over ssh.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive container=docker

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      systemd systemd-sysv dbus openssh-server sudo curl ca-certificates tar git gawk passwd procps nftables \
 && rm -rf /var/lib/apt/lists/* \
 # Units a container has no use for, or that stall its boot.
 && systemctl mask getty.target console-getty.service systemd-logind.service systemd-udevd.service \
      systemd-udev-trigger.service systemd-timesyncd.service apt-daily.timer apt-daily-upgrade.timer \
 && systemctl enable ssh \
 && useradd --create-home --shell /bin/bash deploy \
 && echo 'deploy ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/deploy \
 && chmod 0440 /etc/sudoers.d/deploy \
 && install -d -m 0700 -o deploy -g deploy /home/deploy/.ssh \
 && mkdir -p /run/sshd

STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
