ARG GUEST_RUNTIME_SOURCE=guest-builder
FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc AS guest-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY packages/protocol/proto ./packages/protocol/proto
RUN cargo build --release --locked -p ai-phone-guest-runtime && strip target/release/desktop-bridge
FROM ${GUEST_RUNTIME_SOURCE} AS guest-runtime
FROM oven/bun:1.4.4@sha256:1d22548f177f4ff38e0c15baa3c2c46c6c2060ba2d1bc573bb4720b407548f1d AS bun
FROM python:3.13-slim@sha256:d98cc29d3266e535dd9da1099688d9cb3ca4753813571e071763d17725460962
ARG HTTP_PROXY
ARG HTTPS_PROXY
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git nodejs ripgrep unzip zip poppler-utils && rm -rf /var/lib/apt/lists/*
RUN pip install --no-cache-dir pypdf==6.21.2 python-docx==1.2.0 requests==2.34.2 pillow==13.0.0 openpyxl==3.1.5
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
COPY --from=guest-runtime /build/target/release/desktop-bridge /usr/local/bin/desktop-bridge
RUN /usr/local/bin/desktop-bridge --workspace-job-contract
RUN apt-get update && apt-get install -y --no-install-recommends chromium fonts-noto-cjk && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y --no-install-recommends tesseract-ocr tesseract-ocr-eng tesseract-ocr-chi-sim && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
CMD ["sleep", "infinity"]
