# Stage 1: Build the frontend
FROM node:lts-bookworm-slim AS frontend-builder
WORKDIR /app
COPY . .
WORKDIR /app/html
RUN npm install
RUN npm run build

# Stage 2: Build the backend
FROM rust:bookworm AS backend-builder
WORKDIR /app
COPY --from=frontend-builder /app .
RUN cargo build --release

# Stage 3: Final image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y bash lrzsz vttest && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /app/target/release/ttyd-rs /app/ttyd-rs
EXPOSE 7681
ENTRYPOINT ["/app/ttyd-rs", "--bind-all"]
CMD ["bash"]
