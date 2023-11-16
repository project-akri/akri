BUILD_OPENCV_BASE_VERSION = 0.1.1

#
#
# OPENCV: make and push the open cv intermediate images:
#
#    To make all platforms: `make opencv-base`
#    To make specific platforms: `PLATFORMS="amd64 arm/v7" make opencv-base`
#
#
.PHONY: opencv-base
opencv-base: 
	docker buildx build $(COMMON_DOCKER_BUILD_ARGS) --tag "$(PREFIX)/opencvsharp-build:$(BUILD_OPENCV_BASE_VERSION)" --file $(DOCKERFILE_DIR)/Dockerfile.opencvsharp-build .