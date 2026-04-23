# Stage 1: The Builder
FROM rust:latest AS builder
WORKDIR /usr/src/project-swarm

# Copy the source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Compile the highly optimized release binary
RUN cargo build --release

# Stage 2: The Minimal Runtime
FROM debian:bookworm-slim
WORKDIR /app

# Install necessary root certificates for QUIC/TLS
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Pull the compiled binary from the builder stage
COPY --from=builder /usr/src/project-swarm/target/release/project-swarm-daemon /app/project-swarm-daemon

# Expose the specific UDP port for our P2P QUIC transport
EXPOSE 4001/udp

# Run the daemon
CMD ["./project-swarm-daemon"]