# Maintainer-only factory. Its context contains collect.py and the pinned Python archive.
FROM ubuntu@sha256:78d4744ead1f5b19b04d0df558ab182a5300903900755e515b2ca4d10eb2044d AS factory
COPY bootstrap-ca.deb /tmp/bootstrap-ca.deb
RUN dpkg-deb -x /tmp/bootstrap-ca.deb /tmp/bootstrap-ca && mkdir -p /etc/ssl/certs && cat /tmp/bootstrap-ca/usr/share/ca-certificates/mozilla/*.crt > /etc/ssl/certs/ca-certificates.crt && rm -rf /tmp/bootstrap-ca /tmp/bootstrap-ca.deb
COPY snapshot.conf /etc/apt/apt.conf.d/50aiphone-snapshot
RUN apt-get update --error-on=any && apt-get install -y --no-install-recommends ca-certificates python3-pip pax-utils tesseract-ocr tesseract-ocr-eng tesseract-ocr-chi-sim && rm -rf /var/lib/apt/lists/*
COPY python.tar.gz /tmp/python.tar.gz
RUN echo 'df15fe60d3ff55bf9dcf25824b5572756bb8816ff93865a273d3301d7066a8b7  /tmp/python.tar.gz' | sha256sum -c - && mkdir /portable && tar -xzf /tmp/python.tar.gz -C /portable && rm /tmp/python.tar.gz
RUN mkdir -p /out/python && python3 -m pip download --no-cache-dir --no-deps --only-binary=:all: --platform manylinux_2_28_x86_64 --python-version 313 --implementation cp --abi cp313 --dest /out pillow==13.0.0
COPY collect.py /tmp/collect.py
COPY factory.py /tmp/factory.py
RUN /usr/bin/python3 /tmp/collect.py
FROM scratch AS export
COPY --from=factory /export/ /
