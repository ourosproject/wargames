# A deliberately-weak SSH target with a planted "crown jewel" flag.
FROM debian:12-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      openssh-server iproute2 procps \
 && rm -rf /var/lib/apt/lists/*
ARG DEMO_PW=changeme
RUN useradd -m -s /bin/bash demo \
 && echo "demo:${DEMO_PW}" | chpasswd \
 && mkdir -p /run/sshd /srv/agent/secret \
 && echo 'flag{purple_range_demo_pwned}' > /srv/agent/secret/flag.txt \
 && chmod 644 /srv/agent/secret/flag.txt \
 && sed -ri 's/^#?PasswordAuthentication.*/PasswordAuthentication yes/; s/^#?PermitRootLogin.*/PermitRootLogin no/' /etc/ssh/sshd_config \
 && ssh-keygen -A
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
