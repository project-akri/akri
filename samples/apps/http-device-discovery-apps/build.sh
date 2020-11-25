#!/usr/bin/env bash

: "${REGISTRY:?Need to export REGISTRY e.g. ghcr.io}"
: "${USER:?Need to export USER e.g. ghcr.io/deislabs/...}"
: "${PREFIX:?Need to export PREFIX e.g. ${REGISTRY}/${USER}/akri-http...}"
: "${TAG:?Need to export TAG e.g. latest}"

for APP in "device" "discovery"
do
  IMAGE="${REGISTRY}/${USER}/${PREFIX}-${APP}"
  docker build \
  --tag=${IMAGE}:${TAG} \
  --file=./Dockerfiles/${APP} \
  .
  docker push ${IMAGE}:${TAG}
done

for APP in  "broker" "client"
do
  IMAGE="${REGISTRY}/${USER}/${PREFIX}-grpc-${APP}-golang"
  docker build \
  --tag=${IMAGE}:${TAG} \
  --file=./Dockerfiles/grpc.${APP} \
  .
  docker push ${IMAGE}:${TAG}
done