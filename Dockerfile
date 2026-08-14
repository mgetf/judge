FROM rust:1.88-slim AS builder

WORKDIR /build

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY examples ./examples
COPY config.example.json ./

RUN cargo build --release --bin judge

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/judge /app/judge
COPY config.example.json /app/config.json

RUN mkdir -p /data

ENV JUDGE_BIND=0.0.0.0:8080
ENV JUDGE_DATA_DIR=/data
ENV JUDGE_CONFIG=/app/config.json

EXPOSE 8080

CMD ["/app/judge"]
