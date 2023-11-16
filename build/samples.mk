USE_OPENCV_BASE_VERSION = 0.0.11

.PHONY: samples
samples: opcua-monitoring-broker onvif-video-broker anomaly-detection-app video-streaming-app akri-udev-video-broker

%-app:
	docker buildx build $(COMMON_DOCKER_BUILD_ARGS) --build-arg APPLICATION=$@ --tag "$(PREFIX)/$@:$(LABEL_PREFIX)" --file $(DOCKERFILE_DIR)/Dockerfile.python-app .

opcua-monitoring-broker:
	docker buildx build $(COMMON_DOCKER_BUILD_ARGS) --tag "$(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)" --file $(DOCKERFILE_DIR)/Dockerfile.opcua-monitoring-broker .

# Still use old-ish style for onvif-video-broker as app uses .NET 3.1 that doesn't have multi-arch manifest
onvif-video-broker: onvif-video-broker-multiarch

onvif-video-broker-multiarch: onvif-video-broker-amd64 onvif-video-broker-arm64 onvif-video-broker-arm32
ifeq (1, $(PUSH))
	docker buildx imagetools create --tag "$(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)"
endif

ONVIF_BUILDX_PUSH_OUTPUT = type=image,name=$(PREFIX)/onvif-video-broker,push-by-digest=true,name-canonical=true,push=true
ONVIF_BUILDX_ARGS = $(if $(LOAD), --load --tag $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)) $(if $(PUSH), --output $(ONVIF_BUILDX_PUSH_OUTPUT)) -f $(DOCKERFILE_DIR)/Dockerfile.onvif-video-broker

onvif-video-broker-amd64:
ifneq (,or(findstring(amd64,$(PLATFORMS)), findstring(x86_64,$(PLATFORMS))))
	docker buildx build $(ONVIF_BUILDX_ARGS) $(if $(PUSH), --iidfile onvif-video-broker.sha-amd64) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(AMD64_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-x64 .
endif

onvif-video-broker-arm32:
ifneq (,findstring(arm/v7,$(PLATFORMS)))
	docker buildx build $(ONVIF_BUILDX_ARGS) $(if $(PUSH), --iidfile onvif-video-broker.sha-arm32) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(ARM32V7_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm .
endif

onvif-video-broker-arm64:
ifneq (,or(findstring(aarch64,$(PLATFORMS)),findstring(arm64,$(PLATFORMS))))
	docker buildx build $(ONVIF_BUILDX_ARGS) $(if $(PUSH), --iidfile onvif-video-broker.sha-arm32) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(ARM64V8_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm64 .
endif