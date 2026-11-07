ARG GUEST_RUNTIME_SOURCE=guest-builder
FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc AS guest-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY packages/protocol/proto ./packages/protocol/proto
RUN cargo build --release --locked -p ai-phone-guest-runtime && strip target/release/desktop-bridge
FROM ${GUEST_RUNTIME_SOURCE} AS guest-runtime
FROM oven/bun:1.4.4@sha256:1d22548f177f4ff38e0c15baa3c2c46c6c2060ba2d1bc573bb4720b407548f1d AS bun
FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
ARG HTTP_PROXY
ARG HTTPS_PROXY
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git ripgrep unzip zip poppler-utils chromium fonts-noto-cjk tesseract-ocr tesseract-ocr-eng tesseract-ocr-chi-sim && rm -rf /var/lib/apt/lists/*
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
COPY --from=guest-runtime /build/target/release/desktop-bridge /usr/local/bin/desktop-bridge
RUN /usr/local/bin/desktop-bridge --workspace-job-contract
WORKDIR /workspace
CMD ["sleep", "infinity"]
