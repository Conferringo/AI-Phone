FROM oven/bun:1.4.4@sha256:1d22548f177f4ff38e0c15baa3c2c46c6c2060ba2d1bc573bb4720b407548f1d AS bun
FROM ubuntu@sha256:78d4744ead1f5b19b04d0df558ab182a5300903900755e515b2ca4d10eb2044d AS factory
COPY bootstrap-ca.deb /tmp/bootstrap-ca.deb
RUN dpkg-deb -x /tmp/bootstrap-ca.deb /tmp/bootstrap-ca && mkdir -p /etc/ssl/certs && cat /tmp/bootstrap-ca/usr/share/ca-certificates/mozilla/*.crt > /etc/ssl/certs/ca-certificates.crt && rm -rf /tmp/bootstrap-ca /tmp/bootstrap-ca.deb
COPY ubuntu.sources /etc/apt/sources.list.d/ubuntu.sources
RUN apt-get update --error-on=any && apt-get install -y --no-install-recommends ca-certificates qemu-system-x86 qemu-utils openssh-client seabios ipxe-qemu ovmf && rm -rf /var/lib/apt/lists/*
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
COPY scripts/ /tmp/scripts/
RUN bun /tmp/scripts/linux-host/collect.ts
FROM scratch AS export
COPY --from=factory /export/ /
