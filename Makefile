BUILD_AMD64 ?= 1
BUILD_ARM32 ?= 0 #skip building arm32 for now
BUILD_ARM64 ?= 1

REGISTRY ?= devcaptest.azurecr.io
UNIQUE_ID ?= $(USER)

INTERMEDIATE_DOCKERFILE_DIR ?= build/containers/intermediate
DOCKERFILE_DIR ?= build/containers

PREFIX ?= $(REGISTRY)/$(UNIQUE_ID)
VERSION=$(shell cat version.txt)
VERSION_LABEL=v$(VERSION)
LABEL_PREFIX ?= $(VERSION_LABEL)

CACHE_OPTION ?=

ENABLE_DOCKER_MANIFEST = DOCKER_CLI_EXPERIMENTAL=enabled

AMD64_SUFFIX = amd64
ARM32V7_SUFFIX = arm32v7
ARM64V8_SUFFIX = arm64v8

AMD64_TARGET = x86_64-unknown-linux-gnu
ARM32V7_TARGET = arm-unknown-linux-gnueabihf
ARM64V8_TARGET = aarch64-unknown-linux-gnu

# Intermediate container defines
include build/intermediate-containers.mk

# Akri container defines
include build/akri-containers.mk