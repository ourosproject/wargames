# The attacker box — the command center SSHes here to launch operations.
FROM debian:12-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      openssh-server openssh-client nmap hydra curl ca-certificates iproute2 \
 && rm -rf /var/lib/apt/lists/*
ARG DEMO_PW=changeme
RUN useradd -m -s /bin/bash demo \
 && echo "demo:${DEMO_PW}" | chpasswd \
 && mkdir -p /run/sshd \
 && sed -ri 's/^#?PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config \
 && ssh-keygen -A
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
