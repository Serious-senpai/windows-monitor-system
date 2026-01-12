# Reference: https://docs.docker.com/reference/dockerfile/
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    python3 \
    python-is-python3

RUN curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=1.92 -y

ENV PATH="/root/.cargo/bin:${PATH}"

# https://stackoverflow.com/a/63348989
RUN rustup toolchain install nightly
