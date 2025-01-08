# ---- Stage 1: Build the application ----
FROM rust:latest as builder
WORKDIR /app

COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

# ---- Stage 2: Create a minimal runtime image ----
FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y \
    libssl-dev \
    libssl3 \
    ca-certificates \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/glance-widget-bluesky /app/
COPY .env /app/.env
RUN chmod +x /app/glance-widget-bluesky
EXPOSE 8080
CMD ["/app/glance-widget-bluesky"]