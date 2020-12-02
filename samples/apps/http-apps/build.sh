#!/usr/bin/env bash

: "${REGISTRY:?Need to export REGISTRY e.g. ghcr.io}"
: "${USER:?Need to export USER e.g. ghcr.io/deislabs/...}"
: "${PREFIX:?Need to export PREFIX e.g. ${REGISTRY}/${USER}/http-apps...}"
: "${TAG:?Need to export TAG e.g. v1}"

for APP in "device" "discovery"
do
  IMAGE="${REGISTRY}/${USER}/${PREFIX}-${APP}:${TAG}"
  docker build \
  --tag=${IMAGE} \
  --file=./Dockerfiles/${APP} \
  .
  docker push ${IMAGE}
done

for APP in  "broker" "client"
do
  IMAGE="${REGISTRY}/${USER}/${PREFIX}-grpc-${APP}-golang:${TAG}"
  docker build \
  --tag=${IMAGE} \
  --file=./Dockerfiles/grpc.${APP} \
  .
  docker push ${IMAGE}
done