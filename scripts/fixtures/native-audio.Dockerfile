# Isolated CI fixture, never part of the installed runtime or user device state.
FROM python:3.13-slim@sha256:d98cc29d3266e535dd9da1099688d9cb3ca4753813571e071763d17725460962
RUN apt-get update && apt-get install -y --no-install-recommends pulseaudio pulseaudio-utils ffmpeg procps && command -v pgrep && command -v kill && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 -s /bin/bash ai
COPY --chmod=755 desktop-bridge /usr/local/bin/desktop-bridge
ENV HOME=/home/ai XDG_RUNTIME_DIR=/tmp/ai-runtime PULSE_SERVER=unix:/tmp/ai-runtime/pulse/native
