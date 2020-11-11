#!/usr/bin/env bash

: "${REGISTRY:?Need to export REGISTRY e.g. ghcr.io}"
: "${USER:?Need to export USER e.g. ghcr.io/deislabs/...}"
: "${PREFIX:?Need to export PREFIX e.g. ${REGISTRY}/${USER}/akri-http...}"
: "${TAG:?Need to export TAG e.g. latest}"

# Standalone
(
    IMAGE="${REGISTRY}/${USER}/${PREFIX}-broker"
    docker build \
    --tag=${IMAGE}:${TAG} \
    --file=./Dockerfiles/standalone \
    ../../..

    docker push ${IMAGE}:${TAG}
)

# gRPC Broker|Client
# Broker
(
    IMAGE="${REGISTRY}/${USER}/${PREFIX}-grpc-broker-rust"
    docker build \
    --tag=${IMAGE}:${TAG} \
    --file=./Dockerfiles/grpc.broker \
    ../../..

    docker push ${IMAGE}:${TAG}
)
# Client
(
    IMAGE="${REGISTRY}/${USER}/${PREFIX}-grpc-client-rust"
    docker build \
    --tag=${IMAGE}:${TAG} \
    --file=./Dockerfiles/grpc.client \
    ../../..

    docker push ${IMAGE}:${TAG}
)
