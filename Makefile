BUILD_AMD64 ?= 1
BUILD_ARM32 ?= 1
BUILD_ARM64 ?= 1

# Specify flag to build optimized release version of rust components.
# Set to be empty to use debug builds.
BUILD_RELEASE_FLAG ?= 1

# Space separated list of rust packages to not build such as the following to not build 
# the udev discovery handler library or module: "akri-udev udev-discovery-handler"
PACKAGES_TO_EXCLUDE ?=

# Incremental compilation causes rustc to save additional information to disk which will be 
# reused when recompiling the crate, improving re-compile times. 
# The additional information is stored in the target directory.
# By default for cargo builds, it is enabled in debug mode and disabled in release mode.
CARGO_INCREMENTAL ?= 0

BUILD_SLIM_AGENT ?= 1
FULL_AGENT_EXECUTABLE_NAME ?= agent-full
# Specify which features of the Agent to build, namely which Discovery Handlers
# should be embedded if any. The "agent-full" feature must be enabled to use the embedded
# Discovery Handlers. IE: AGENT_FEATURES="agent-full onvif-feat opcua-feat udev-feat"
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

AMD64_TARGET = x86_64-unknown-linux-gnu
ARM32V7_TARGET = armv7-unknown-linux-gnueabihf
ARM64V8_TARGET = aarch64-unknown-linux-gnu

# Intermediate container defines
include build/intermediate-containers.mk

# Akri container defines
include build/akri-containers.mk
