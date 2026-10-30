FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y systemd systemd-sysv openssh-server sudo curl ca-certificates unzip git && rm -rf /var/lib/apt/lists/*
RUN id ubuntu >/dev/null 2>&1 || useradd -m -s /bin/bash ubuntu && echo 'ubuntu ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/ubuntu && mkdir -p /home/ubuntu/.ssh && chmod 700 /home/ubuntu/.ssh
COPY key.pub /home/ubuntu/.ssh/authorized_keys
RUN chown -R ubuntu:ubuntu /home/ubuntu/.ssh && chmod 600 /home/ubuntu/.ssh/authorized_keys && systemctl enable ssh
# logind cannot start inside the container, so pam_systemd made every SSH session wait 25 s for it, longer than
# Atlas Desktop's 20 s connect probe ("… is not connected"). A real server has logind; this container doesn't need it.
RUN sed -i '/pam_systemd.so/d' /etc/pam.d/common-session
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
