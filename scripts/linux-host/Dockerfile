FROM ubuntu@sha256:78d4744ead1f5b19b04d0df558ab182a5300903900755e515b2ca4d10eb2044d AS factory
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates python3 pax-utils qemu-system-x86 qemu-utils openssh-client seabios ipxe-qemu && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y --no-install-recommends ovmf && rm -rf /var/lib/apt/lists/*
COPY collect.py /tmp/collect.py
COPY factory.py /tmp/factory.py
RUN /usr/bin/python3 /tmp/collect.py
FROM scratch AS export
COPY --from=factory /export/ /
