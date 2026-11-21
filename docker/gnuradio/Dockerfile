FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    gnuradio \
    python3-numpy \
    python3-scipy \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
