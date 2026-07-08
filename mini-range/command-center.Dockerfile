# The command center — serves the dashboard and drives the range over SSH.
FROM node:20-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      openssh-client expect ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY command-center ./command-center
COPY bin ./bin
RUN chmod +x bin/sshpass.exp command-center/server.js
EXPOSE 4899
# range.config.json is mounted at runtime by docker-compose.
CMD ["node", "command-center/server.js"]
