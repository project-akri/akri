#!/usr/bin/env bash

: "${REGISTRY:?Need to export REGISTRY e.g. ghcr.io}"
: "${USER:?Need to export USER e.g. ghcr.io/deislabs/...}"
: "${PREFIX:?Need to export PREFIX e.g. ${REGISTRY}/${USER}/http...}"
: "${TAG:?Need to export TAG e.g. v1}"

# Standalone
(
    IMAGE="${REGISTRY}/${USER}/${PREFIX}-broker:${TAG}"
    docker build \
    --tag=${IMAGE} \
    --file=./Dockerfiles/standalone \
    ../../..

    docker push ${IMAGE}
)

# gRPC Broker|Client
# Broker
(
    IMAGE="${REGISTRY}/${USER}/${PREFIX}-grpc-broker:${TAG}"
    docker build \
    --tag=${IMAGE} \
    --file=./Dockerfiles/grpc.broker \
    ../../..

    docker push ${IMAGE}
)
# Client
(
    IMAGE="${REGISTRY}/${USER}/${PREFIX}-grpc-client:${TAG}"
    docker build \
    --tag=${IMAGE} \
    --file=./Dockerfiles/grpc.client \
    ../../..

    docker push ${IMAGE}
)
