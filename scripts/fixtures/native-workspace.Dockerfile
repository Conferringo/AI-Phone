# Isolated native-worker qualification; not an end-user workspace image.
ARG WORKSPACE_TEST_BASE=ubuntu:24.04@sha256:fe81560990fae86dc4743202b2cb4d3df98d0bcf949173cecef15ce1127ef852
FROM ${WORKSPACE_TEST_BASE}
COPY --chmod=755 desktop-bridge /usr/local/bin/desktop-bridge
WORKDIR /workspace
CMD ["sleep", "infinity"]
