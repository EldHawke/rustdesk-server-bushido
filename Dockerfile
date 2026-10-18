# syntax=docker/dockerfile:1.7

# ---------- Builder ----------
FROM rust:1.89-trixie AS builder

WORKDIR /build

COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --bins

# ---------- Runtime ----------
FROM debian:trixie-slim

ARG S6_OVERLAY_VERSION=3.2.0.0
ARG S6_ARCH=x86_64

ADD https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-noarch.tar.xz /tmp
ADD https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-${S6_ARCH}.tar.xz /tmp

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        libgcc-s1 \
        ca-certificates \
        xz-utils && \
    tar -C / -Jxpf /tmp/s6-overlay-noarch.tar.xz && \
    tar -C / -Jxpf /tmp/s6-overlay-${S6_ARCH}.tar.xz && \
    rm -rf /var/lib/apt/lists/* /tmp/*.tar.xz && \
    ln -s /run /var/run

COPY docker/rootfs /

COPY --from=builder /build/target/release/hbbs /usr/bin/hbbs
COPY --from=builder /build/target/release/hbbr /usr/bin/hbbr
COPY --from=builder /build/target/release/rustdesk-utils /usr/bin/rustdesk-utils

ENV RELAY=relay.example.com
ENV ENCRYPTED_ONLY=0

EXPOSE 21115 21116 21116/udp 21117 21118 21119

HEALTHCHECK --interval=10s --timeout=5s CMD /usr/bin/healthcheck.sh

WORKDIR /data
VOLUME /data

ENTRYPOINT ["/init"]