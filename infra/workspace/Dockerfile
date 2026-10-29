FROM oven/bun:1.4.4@sha256:1d22548f177f4ff38e0c15baa3c2c46c6c2060ba2d1bc573bb4720b407548f1d AS bun
FROM python:3.13-slim@sha256:d98cc29d3266e535dd9da1099688d9cb3ca4753813571e071763d17725460962
ARG HTTP_PROXY
ARG HTTPS_PROXY
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git nodejs ripgrep unzip zip poppler-utils && rm -rf /var/lib/apt/lists/*
RUN pip install --no-cache-dir pypdf==6.21.2 python-docx==1.2.0 requests==2.34.2 pillow==13.0.0 openpyxl==3.1.5
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends chromium fonts-noto-cjk && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
CMD ["sleep", "infinity"]
