FROM ai-phone-workspace:20261026
ARG HTTP_PROXY
ARG HTTPS_PROXY
USER root
RUN sed -i 's|http://deb.debian.org|https://deb.debian.org|g' /etc/apt/sources.list.d/debian.sources
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::Retries=4 install -y --no-install-recommends xvfb openbox tint2 pcmanfm xterm xdotool xclip x11-utils x11-xserver-utils dbus-x11 ffmpeg libreoffice-calc libreoffice-writer libreoffice-l10n-zh-cn fonts-noto-cjk mousepad && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 -s /bin/bash ai && mkdir -p /home/ai/Files /home/ai/Desktop && chown -R ai:ai /home/ai
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::Retries=4 install -y --no-install-recommends pulseaudio pulseaudio-utils sudo && rm -rf /var/lib/apt/lists/*
RUN printf 'ai ALL=(ALL:ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/ai-phone && chmod 440 /etc/sudoers.d/ai-phone
COPY bridge.py /usr/local/bin/desktop-bridge
COPY start.sh /usr/local/bin/desktop-start
COPY media-capture.py audio-service.py /usr/local/lib/ai-phone/
COPY pulse.pa /etc/ai-phone-pulse.pa
RUN chmod 755 /usr/local/bin/desktop-bridge /usr/local/bin/desktop-start
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends iproute2 iptables && rm -rf /var/lib/apt/lists/*
COPY --from=clash_apps . /tmp/clash-app/
RUN apt-get -o Acquire::Retries=4 update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends /tmp/clash-app/*.deb libglib2.0-bin gsettings-desktop-schemas && rm -rf /var/lib/apt/lists/* /tmp/clash-app
COPY clash-launch.sh /usr/local/bin/ai-phone-clash
RUN chmod 755 /usr/local/bin/ai-phone-clash
COPY network-start.sh /usr/local/bin/device-network-start
RUN chmod 755 /usr/local/bin/device-network-start
ENV DISPLAY=:0 LANG=C.UTF-8 HOME=/home/ai XDG_RUNTIME_DIR=/tmp/ai-runtime PULSE_SERVER=unix:/tmp/ai-runtime/pulse/native
WORKDIR /home/ai/Files
ENTRYPOINT ["/usr/local/bin/desktop-start"]
