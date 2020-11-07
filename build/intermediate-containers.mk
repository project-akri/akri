
BUILD_RUST_CROSSBUILD_VERSION = 0.0.7

BUILD_OPENCV_BASE_VERSION = 0.0.7

CROSS_VERSION = 0.1.16

#
#
# OPENCV: make and push the open cv intermediate images:
#
#    To make all platforms: `make opencv-base`
#    To make specific platforms: `BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=1 make opencv`
#
#
.PHONY: opencv-base
opencv-base: opencv-base-build opencv-base-docker-per-arch
opencv-base-build: opencv-base-build-amd64 opencv-base-build-arm32 opencv-base-build-arm64
opencv-base-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(INTERMEDIATE_DOCKERFILE_DIR)/Dockerfile.opencvsharp-build . -t $(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)-$(AMD64_SUFFIX) --build-arg PLATFORM_TAG=3.1-buster-slim
endif
opencv-base-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(INTERMEDIATE_DOCKERFILE_DIR)/Dockerfile.opencvsharp-build . -t $(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)-$(ARM32V7_SUFFIX) --build-arg PLATFORM_TAG=3.1-buster-slim-$(ARM32V7_SUFFIX)
endif
opencv-base-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(INTERMEDIATE_DOCKERFILE_DIR)/Dockerfile.opencvsharp-build . -t $(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)-$(ARM64V8_SUFFIX) --build-arg PLATFORM_TAG=3.1-buster-slim-$(ARM64V8_SUFFIX)
endif
opencv-base-docker-per-arch: opencv-base-docker-per-arch-amd64 opencv-base-docker-per-arch-arm32 opencv-base-docker-per-arch-arm64
opencv-base-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)-$(AMD64_SUFFIX)
endif
opencv-base-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)-$(ARM32V7_SUFFIX)
endif
opencv-base-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)-$(ARM64V8_SUFFIX)
endif

#
#
# CROSS: make and push the intermediate images for the cross building Rust:
#
#    To make all platforms: `make rust-crossbuild`
#    To make specific platforms: `BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=1 make rust-crossbuild`
#
#
.PHONY: rust-crossbuild
rust-crossbuild: rust-crossbuild-build rust-crossbuild-docker-per-arch
rust-crossbuild-build: rust-crossbuild-build-amd64 rust-crossbuild-build-arm32 rust-crossbuild-build-arm64
rust-crossbuild-build-amd64:
ifeq (1, $(BUILD_AMD64))
	 docker build $(CACHE_OPTION) -f $(INTERMEDIATE_DOCKERFILE_DIR)/Dockerfile.rust-crossbuild-$(AMD64_SUFFIX) . -t $(PREFIX)/rust-crossbuild:$(AMD64_TARGET)-$(CROSS_VERSION)-$(BUILD_RUST_CROSSBUILD_VERSION)
endif
rust-crossbuild-build-arm32:
ifeq (1, ${BUILD_ARM32})
	 docker build $(CACHE_OPTION) -f $(INTERMEDIATE_DOCKERFILE_DIR)/Dockerfile.rust-crossbuild-$(ARM32V7_SUFFIX) . -t $(PREFIX)/rust-crossbuild:$(ARM32V7_TARGET)-$(CROSS_VERSION)-$(BUILD_RUST_CROSSBUILD_VERSION)
endif
rust-crossbuild-build-arm64:
ifeq (1, ${BUILD_ARM64})
	 docker build $(CACHE_OPTION) -f $(INTERMEDIATE_DOCKERFILE_DIR)/Dockerfile.rust-crossbuild-$(ARM64V8_SUFFIX) . -t $(PREFIX)/rust-crossbuild:$(ARM64V8_TARGET)-$(CROSS_VERSION)-$(BUILD_RUST_CROSSBUILD_VERSION)
endif

rust-crossbuild-docker-per-arch: rust-crossbuild-docker-per-arch-amd64 rust-crossbuild-docker-per-arch-arm32 rust-crossbuild-docker-per-arch-arm64
rust-crossbuild-docker-per-arch-amd64:
ifeq (1, $(BUILD_AMD64))
	 docker push $(PREFIX)/rust-crossbuild:$(AMD64_TARGET)-$(CROSS_VERSION)-$(BUILD_RUST_CROSSBUILD_VERSION)
endif
rust-crossbuild-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	 docker push $(PREFIX)/rust-crossbuild:$(ARM32V7_TARGET)-$(CROSS_VERSION)-$(BUILD_RUST_CROSSBUILD_VERSION)
endif
rust-crossbuild-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	 docker push $(PREFIX)/rust-crossbuild:$(ARM64V8_TARGET)-$(CROSS_VERSION)-$(BUILD_RUST_CROSSBUILD_VERSION)
endif
