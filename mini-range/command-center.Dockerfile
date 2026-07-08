# The command center — serves the dashboard and drives the range over SSH.
# Multi-stage: build the Rust binary, then ship it in a tiny runtime image.

# Pin to bookworm so the build glibc matches the debian:12-slim runtime below
# (a bare rust:1-slim tracks trixie and links against a newer glibc the runtime lacks).
FROM rust:1-slim-bookworm AS build
WORKDIR /build
COPY command-center/Cargo.toml command-center/Cargo.lock ./
COPY command-center/src ./src
COPY command-center/public ./public
RUN cargo build --release

FROM debian:12-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      openssh-client expect ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /build/target/release/purple-range /usr/local/bin/purple-range
COPY bin ./bin
RUN chmod +x bin/sshpass.exp
EXPOSE 4899
# range.config.json is mounted at runtime by docker-compose.
CMD ["purple-range"]
