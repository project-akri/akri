ARG PLATFORM=amd64
ARG CROSS_BUILD_TARGET=x86_64-unknown-linux-gnu
ARG BROKER=http

FROM ${PLATFORM}/rust:1.47 as build

ARG BROKER

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    g++ \
    ca-certificates \
    curl \
    libssl-dev \
    pkg-config

RUN rustup component add rustfmt --toolchain 1.47.0-x86_64-unknown-linux-gnu

WORKDIR /${BROKER}

RUN echo '[workspace]' > ./Cargo.toml && \
    echo 'members = ["shared", "samples/brokers/http"]' >> ./Cargo.toml

COPY ./samples/brokers/${BROKER} ./samples/brokers/${BROKER}
COPY ./shared ./shared

RUN cargo build --release

FROM amd64/debian:buster-slim

ARG BROKER

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl-dev \
    openssl && \
    apt-get clean

# Rename ${BROKER} binary to broker
COPY --from=build /${BROKER}/target/release/${BROKER} /broker

LABEL org.opencontainers.image.source https://github.com/dazwilkin/akri

# Expose port used by broker service
EXPOSE 8083

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs
ENV RUST_LOG ${BROKER},akri_shared

ENTRYPOINT ["/broker"]
