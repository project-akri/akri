# Building

Building Akri, whether locally or in the automated CI builds, leverages the same set of `make` commands.

In essence, Akri components can be thought of as: 1. Runtime components 1. Rust code: containers based on Rust code are built using `Cargo cross` and subsequent `docker build` commands include the cross-built binaries.

{% hint style="info" %}
 For Rust code, build/Dockerfile. does NOT run cargo build, instead they simply copy cross-built binaries into the container 2. Other code: these containers can be .NET or python or whatever else ... the build/Dockerfile. must do whatever building is required. 2. Intermediate components: these containers are used as part of the build process and are not used in production explicitly
{% endhint %}

## Runtime components

The Akri runtime components are the containers that provide Akri's functionality. They include the agent, the controller, the webhook, the brokers, and the applications. The majority of Akri runtime components are written in Rust, but there are several components that are written in .NET or python.

All of the runtime components are built with a `make` command. These are the supporting makefiles:

* `Makefile`: this provides a single point of entry to build any Akri component
* `build/akri-containers.mk`: this provides the build and push functionality for Akri containers
* `build/akri-rust-containers.mk`: this provides a simple definition to build and push Akri components written in Rust
* `build/akri-dotnet-containers.mk`: this provides a simple definition to build and push Akri components written in .NET
* `build/akri-python-containers.mk`: this provides a simple definition to build and push Akri components written in Python

### Configurability

The makefiles allow for several configurations:

* BUILD\_AMD64: if set not to 1, the make commands will ignore AMD64
* BUILD\_ARM32: if set not to 1, the make commands will ignore ARM32
* BUILD\_ARM64: if set not to 1, the make commands will ignore ARM64
* REGISTRY: allows configuration of the container registry \(defaults to imaginary: devcaptest.azurecr.io\)
* UNIQUE\_ID: allows configuration of container registry account \(defaults to $USER\)
* PREFIX: allows configuration of container registry path for containers
* LABEL\_PREFIX: allows configuration of container labels
* CACHE\_OPTION: when `CACHE_OPTION=--no-cache`, the `docker build` commands will not use local caches

### Local development usage

For a local build, some typical patterns are:

* `make akri-build`: run Rust cross-build for all platforms
* `BUILD_AMD64=0 BUILD_ARM32=0 BUILD_ARM64=1 make akri-build`: run Rust cross-build for ARM64
* `PREFIX=ghcr.io/myaccount make akri`: builds all of the Akri containers and stores them in a container registry, `ghcr.io/myaccount`.
* `PREFIX=ghcr.io/myaccount make akri`: builds all of the Akri containers and stores them in a container registry, `ghcr.io/myaccount`.
* `PREFIX=ghcr.io/myaccount LABEL_PREFIX=local make akri`: builds all of the Akri containers and stores them in a container registry, `ghcr.io/myaccount` with labels prefixed with `local`.
* `PREFIX=ghcr.io/myaccount BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri`: builds all of the Akri containers for AMD64 and stores them in a container registry, `ghcr.io/myaccount`.
* `PREFIX=ghcr.io/myaccount make akri-controller`: builds the Akri controller container for all platforms and stores them in a container registry, `ghcr.io/myaccount`.

### make targets

For each component, there will be a common set of targets:

* `akri-<component>`: this target will cross-build Akri and build+push this component's container for all platforms
* `akri-docker-<component>`: this target will build+push this component's container for all platforms
* `<component>-build`: this target will build this component's container for all platforms
* `<component>-build-amd64`: this target will build this component's container for amd64
* `<component>-build-arm32`: this target will build this component's container for arm32
* `<component>-build-arm64`: this target will build this component's container for arm64
* `<component>-docker-per-arch`: this target will push this component's container for all platforms
* `<component>-docker-per-arch-amd64`: this target will push this component's container for amd64
* `<component>-docker-per-arch-arm32`: this target will push this component's container for arm32
* `<component>-docker-per-arch-arm64`: this target will push this component's container for arm64
* `<component>-docker-multi-arch-create`: this target will create a multi-arch manifest for this component and include all platforms
* `<component>-docker-multi-arch-push`: this target will push a multi-arch manifest for this component

### Adding a new component

To add a new Rust-based component, follow these steps \(substituting the new component name for `<new-component>`\): 1. Add `$(eval $(call add_rust_targets,<new-component>,<new-component>))` to `build/akri-containers.mk` 1. Create `build/Dockerfile.<new-component>`

> A simple way to do this is to copy `build/Dockerfile.agent` and replace `agent` with whatever `<new-component>` is. 1. Create `.github/workflows/build-<new-component>-container.yml` A simple way to do this is to copy `.github/workflows/build-agent-container.yml` and replace `agent` with whatever `<new-component>` is.

## Intermediate components

These are the intermediate components:

* [rust-crossbuild](https://github.com/orgs/deislabs/packages/container/package/akri%2Frust-crossbuild)
* [opencvsharp-build](https://github.com/orgs/deislabs/packages/container/package/akri%2Fopencvsharp-build)

### rust-crossbuild

This container is used by the Akri cross-build process. The main purpose of these containers is to provide `Cargo cross` with a Rust build environment that has all the required dependencies installed. This container can be built locally for all platforms using this command:

```bash
BUILD_AMD64=1 BUILD_ARM32=1 BUILD_AMD64=1 make rust-crossbuild
```

If a change needs to be made to this container, 2 pull requests are needed. 1. Create PR with desired `rust-crossbuild` changes \(new dependencies, etc\) AND update `BUILD_RUST_CROSSBUILD_VERSION` in `build/intermediate-containers.mk`. This PR is intended to create the new version of `rust-crossbuild` \(not to use it\). 1. After 1st PR is merged and the new version of `rust-crossbuild` is pushed to ghcr.io/akri, create PR with any changes that will leverage the new version of `rust-crossbuild` AND update `Cross.toml` \(the `BUILD_RUST_CROSSBUILD_VERSION` value specified in step 1 should be each label's suffix\). This PR is intended to **use** the new version of `rust-crossbuild`.

### opencvsharp-build

This container is used by the [onvif-video-broker](https://github.com/orgs/deislabs/packages/container/package/akri%2Fonvif-video-broker) as part of its build process. The main purpose of this container is to prevent each build from needing to build the OpenCV C\# platform. This container can be built locally for all platforms using this command:

```bash
BUILD_AMD64=1 BUILD_ARM32=1 BUILD_AMD64=1 make opencv-base
```

If a change needs to be made to this container, 2 pull requests are needed. 1. Create PR with desired `opencvsharp-build` changes \(new dependencies, etc\) AND update `BUILD_OPENCV_BASE_VERSION` in `build/intermediate-containers.mk`. This PR is intended to create the new version of `opencvsharp-build` \(not to use it\). 1. After 1st PR is merged and the new version of `opencvsharp-build` is pushed to ghcr.io/akri, create PR with any changes that will leverage the new version of `opencvsharp-build` AND update `USE_OPENCV_BASE_VERSION` in `build/akri-containers.mk`. This PR is intended to **use** the new version of `opencvsharp-build`.

## Automated builds usage

The automated CI builds essentially run these commands, where `<component>` is one of \(`controller`\|`agent`\|`udev`\|`webhook-configuration`\|`onvif`\|`opcua-monitoring`\|`anomaly-detection`\|`streaming`\) and `<platform>` is one of \(`amd64`\|`arm32`\|`arm64`\):

```bash
# Install the Rust cross building tools
make install-cross
# Cross-builds Rust code for specified platform
make akri-cross-build-<platform>
# Cross-builds Rust code for specified component and platform
make <component>-build-<platform>
# Create container for specified component and platform using versioned label
LABEL_PREFIX="v$(cat version.txt)-dev" make <component>-build-<platform>
# Create container for specified component and platform using latest label
LABEL_PREFIX=`latest-dev` make <component>-build-<platform>

PREFIX=`ghcr.io/deislabs`
# Push container for specified component and platform with versioned label to container registry
LABEL_PREFIX="v$(cat version.txt)-dev" make <component>-docker-per-arch-<platform>
# Push container for specified component and platform with latest label to container registry
LABEL_PREFIX=`latest-dev` make <component>-docker-per-arch-<platform>

DOCKER_CLI_EXPERIMENTAL=`enabled`
PREFIX=`ghcr.io/deislabs`
# Create manifest for multi-arch versioned container
LABEL_PREFIX="v$(cat version.txt)-dev" make <component>-docker-multi-arch-create
# Push manifest for multi-arch versioned container
LABEL_PREFIX="v$(cat version.txt)-dev" make <component>-docker-multi-arch-push
# Create manifest for multi-arch latest container
LABEL_PREFIX=`latest-dev` make <component>-docker-multi-arch-create
# Push manifest for multi-arch latest container
LABEL_PREFIX=`latest-dev` make <component>-docker-multi-arch-push
```

## Build and run Akri without a Container Registry

For development and/or testing, it can be convenient to run Akri without a Container Registry. For example, the Akri CI tests that validate pull requests build Akri components locally, store the containers only in local docker, and configure Helm to only use the local docker containers.

There are two steps to this. For the sake of this demonstration, only the amd64 version of the agent and controller will be built, but this method can be extended to any and all components: 1. Build:

```bash
    # Only build AMD64
    BUILD_AMD64=1
    # PREFIX can be anything, as long as it matches what is specified in the Helm command
    PREFIX=no-container-registry
    # LABEL_PREFIX can be anything, as long as it matches what is specified in the Helm command
    LABEL_PREFIX=dev
    # Build the Rust code
    make akri-build
    # Build the controller container locally for amd64
    make controller-build-amd64
    # Build the agent container locally for amd64
    make agent-build-amd64
```

1. Runtime

   ```bash
    # Specify pullPolicy as Never
    # Specify repository as $PREFIX/<component>
    # Specify tag as $LABEL_PREFIX-amd64
    helm install akri ./deployment/helm \
        --set agent.image.pullPolicy=Never \
        --set agent.image.repository="$PREFIX/agent" \
        --set agent.image.tag="$LABEL_PREFIX-amd64" \
        --set controller.image.pullPolicy=Never \
        --set controller.image.repository="$PREFIX/controller" \
        --set controller.image.tag="$LABEL_PREFIX-amd64"
   ```

