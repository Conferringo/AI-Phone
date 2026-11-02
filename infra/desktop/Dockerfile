ARG WORKSPACE_IMAGE=ai-phone-workspace:20261026
ARG GUEST_RUNTIME_SOURCE=guest-builder
FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc AS guest-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY packages/protocol/proto ./packages/protocol/proto
RUN cargo build --release --locked -p ai-phone-guest-runtime && strip target/release/desktop-bridge
FROM ${GUEST_RUNTIME_SOURCE} AS guest-runtime
FROM ${WORKSPACE_IMAGE}
ARG HTTP_PROXY
ARG HTTPS_PROXY
USER root
RUN sed -i 's|http://deb.debian.org|https://deb.debian.org|g' /etc/apt/sources.list.d/debian.sources
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::Retries=4 install -y --no-install-recommends xvfb openbox tint2 pcmanfm xterm xdotool xclip x11-utils x11-xserver-utils dbus-x11 ffmpeg libreoffice-calc libreoffice-writer libreoffice-l10n-zh-cn fonts-noto-cjk mousepad file && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 -s /bin/bash ai && mkdir -p /home/ai/Files /home/ai/Desktop && chown -R ai:ai /home/ai
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::Retries=4 install -y --no-install-recommends pulseaudio pulseaudio-utils sudo dunst libnotify-bin && rm -rf /var/lib/apt/lists/*
RUN printf 'ai ALL=(ALL:ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/ai-phone && chmod 440 /etc/sudoers.d/ai-phone
COPY infra/desktop/location.js /usr/local/lib/ai-phone/location.js
COPY --from=guest-runtime /build/target/release/desktop-bridge /usr/local/bin/desktop-bridge
RUN /usr/local/bin/desktop-bridge --version
COPY LICENSE NOTICE docs/public/native-services-licenses.txt docs/public/native-services-license-inventory.json /usr/share/doc/ai-phone/
COPY infra/desktop/start.sh /usr/local/bin/desktop-start
COPY infra/desktop/home-initialize.sh /usr/local/lib/ai-phone/
COPY infra/desktop/dunstrc /etc/xdg/dunst/dunstrc
RUN ln -s /usr/local/bin/desktop-bridge /usr/local/bin/desktop-notification-record
COPY infra/desktop/pulse.pa /etc/ai-phone-pulse.pa
RUN chmod 755 /usr/local/bin/desktop-bridge /usr/local/bin/desktop-start
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends iproute2 iptables && rm -rf /var/lib/apt/lists/*
RUN --mount=from=clash_apps,target=/tmp/clash-app,readonly apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends /tmp/clash-app/*.deb libglib2.0-bin gsettings-desktop-schemas && rm -rf /var/lib/apt/lists/*
COPY infra/desktop/clash-launch.sh /usr/local/bin/ai-phone-clash
RUN chmod 755 /usr/local/bin/ai-phone-clash
COPY infra/desktop/network-start.sh /usr/local/bin/device-network-start
RUN chmod 755 /usr/local/bin/device-network-start
ENV DISPLAY=:0 LANG=C.UTF-8 HOME=/home/ai XDG_RUNTIME_DIR=/tmp/ai-runtime PULSE_SERVER=unix:/tmp/ai-runtime/pulse/native
COPY infra/desktop/mimeapps.list /etc/xdg/mimeapps.list
RUN command -v file && \
    test "$(xdg-mime query default text/plain)" = org.xfce.mousepad.desktop && \
    test "$(xdg-mime query default text/markdown)" = org.xfce.mousepad.desktop && \
    probe="$(mktemp --suffix=.md)" && printf '# Desktop text probe\n' > "$probe" && \
    xdg-mime query filetype "$probe" && rm "$probe"
WORKDIR /home/ai
ENTRYPOINT ["/usr/local/bin/desktop-start"]
