#!/usr/bin/env bash

: "${REGISTRY:?Need to set REGISTRY e.g. ghcr.io}"
: "${USER:?Need to set USER e.g. ghcr.io/deislabs/...}"
: "${IMAGE:?Need to set IMAGE e.g. ghcr.io/deislabs/akri/http}"
: "${TAG:?Need to set TAG e.g. latest}"

docker build \
--tag=${REGISTRY}/${USER}/${IMAGE}:${TAG} \
--file=./Dockerfile \
../../..

docker push ${REGISTRY}/${USER}/${IMAGE}:${TAG}
