BUILDX_ARGS =
EXTRA_CARGO_ARGS =

PUSH ?= 
LOAD ?=
LOCAL_ARCH = $(shell uname -m)
ifeq ($(LOAD), 1)
PLATFORMS ?= $(LOCAL_ARCH)
ifneq (1, $(words $(PLATFORMS)))
$(error Cannot load for more than one platform: [$(PLATFORMS)])
endif
else
PLATFORMS ?= amd64 arm64 arm/v7
endif

null  :=
space := $(null) #
comma := ,

DOCKER_PLATFORMS = $(subst $(space),$(comma),$(strip $(addprefix linux/, $(PLATFORMS))))

# Specify flag to build optimized release version of rust components.
# Set to be empty to use debug builds.
BUILD_RELEASE_FLAG ?= 1

# Specify which features of the Agent to build, namely which Discovery Handlers
# should be embedded if any. IE: AGENT_FEATURES="onvif-feat opcua-feat udev-feat"
AGENT_FEATURES ?=

REGISTRY ?= devcaptest.azurecr.io
UNIQUE_ID ?= $(USER)

INTERMEDIATE_DOCKERFILE_DIR ?= build/containers/intermediate
DOCKERFILE_DIR ?= build/containers

PREFIX ?= $(REGISTRY)/$(UNIQUE_ID)

# Evaluate VERSION and TIMESTAMP immediately to avoid
# any lazy evaluation change in the values
VERSION := $(shell cat version.txt)
TIMESTAMP := $(shell date +"%Y%m%d_%H%M%S")

VERSION_LABEL=v$(VERSION)-$(TIMESTAMP)
LABEL_PREFIX ?= $(VERSION_LABEL)

CACHE_OPTION ?=

ENABLE_DOCKER_MANIFEST = DOCKER_CLI_EXPERIMENTAL=enabled

AMD64_SUFFIX = amd64
ARM32V7_SUFFIX = arm32v7
ARM64V8_SUFFIX = arm64v8

COMMON_DOCKER_BUILD_ARGS = $(if $(LOAD), --load) $(if $(PUSH), --push) --platform=$(DOCKER_PLATFORMS) 
# Intermediate container defines
include build/intermediate-containers.mk

# Akri container defines
include build/akri-containers.mk
include build/samples.mk

.PHONY: all
all: akri samples opencv-base

.PHONY: push
push: PUSH = 1
push: all

.PHONY: load
load: LOAD = 1
load: all