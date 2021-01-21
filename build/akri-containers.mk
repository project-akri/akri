USE_OPENCV_BASE_VERSION = 0.0.7

#
#
# INSTALL-CROSS: install cargo cross building tool:
#
#    `make install-cross`
#
#
.PHONY: install-cross
install-cross:
	cargo install cross


#
#
# AKRI: make and push the images for akri:
#
#    To make all platforms: `make akri`
#    To make specific platforms: `BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=1 make akri`
#    To make single component: `make akri-[controller|agent|udev|coap-core|onvif|streaming|opcua-monitoring|anomaly-detection]`
#    To make specific platforms: `BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=1 make akri-[controller|agent|udev|onvif|streaming|opcua-monitoring|anomaly-detection]`
#
#
.PHONY: akri
akri: akri-build akri-docker
akri-controller: akri-build akri-docker-controller
akri-agent: akri-build akri-docker-agent
akri-udev: akri-build akri-docker-udev
akri-coap-core: akri-build akri-docker-coap-core
akri-onvif: akri-build akri-docker-onvif
akri-streaming: akri-build akri-docker-streaming
akri-opcua-monitoring: akri-docker-opcua-monitoring
akri-anomaly-detection: akri-docker-anomaly-detection

akri-build: install-cross akri-cross-build
akri-docker: akri-docker-build akri-docker-push-per-arch akri-docker-push-multi-arch-create akri-docker-push-multi-arch-push
akri-docker-controller: controller-build controller-docker-per-arch controller-docker-multi-arch-create controller-docker-multi-arch-push
akri-docker-agent: agent-build agent-docker-per-arch agent-docker-multi-arch-create agent-docker-multi-arch-push
akri-docker-udev: udev-build udev-docker-per-arch udev-docker-multi-arch-create udev-docker-multi-arch-push
akri-docker-coap-core: coap-core-build coap-core-docker-per-arch coap-core-docker-multi-arch-create coap-core-docker-multi-arch-push
akri-docker-onvif: onvif-build onvif-docker-per-arch onvif-docker-multi-arch-create onvif-docker-multi-arch-push
akri-docker-streaming: streaming-build streaming-docker-per-arch streaming-docker-multi-arch-create streaming-docker-multi-arch-push
akri-docker-opcua-monitoring: opcua-monitoring-build  opcua-monitoring-docker-per-arch opcua-monitoring-docker-multi-arch-create opcua-monitoring-docker-multi-arch-push
akri-docker-anomaly-detection: anomaly-detection-build anomaly-detection-docker-per-arch anomaly-detection-docker-multi-arch-create anomaly-detection-docker-multi-arch-push

akri-cross-build: akri-cross-build-amd64 akri-cross-build-arm32 akri-cross-build-arm64
akri-cross-build-amd64:
ifeq (1, $(BUILD_AMD64))
	PKG_CONFIG_ALLOW_CROSS=1 cross build --release --target=$(AMD64_TARGET)
endif
akri-cross-build-arm32:
ifeq (1, ${BUILD_ARM32})
	PKG_CONFIG_ALLOW_CROSS=1 cross build --release --target=$(ARM32V7_TARGET)
endif
akri-cross-build-arm64:
ifeq (1, ${BUILD_ARM64})
	PKG_CONFIG_ALLOW_CROSS=1 cross build --release --target=$(ARM64V8_TARGET)
endif

akri-docker-build: controller-build agent-build udev-build coap-core-build onvif-build streaming-build opcua-monitoring-build anomaly-detection-build
controller-build: controller-build-amd64 controller-build-arm32 controller-build-arm64
controller-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.controller . -t $(PREFIX)/controller:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(AMD64_TARGET)
endif
controller-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.controller . -t $(PREFIX)/controller:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM32V7_TARGET)
endif
controller-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.controller . -t $(PREFIX)/controller:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM64V8_TARGET)
endif

agent-build: agent-build-amd64 agent-build-arm32 agent-build-arm64
agent-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.agent . -t $(PREFIX)/agent:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(AMD64_TARGET)
endif
agent-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.agent . -t $(PREFIX)/agent:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM32V7_TARGET)
endif
agent-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.agent . -t $(PREFIX)/agent:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM64V8_TARGET)
endif

udev-build: udev-build-amd64 udev-build-arm32 udev-build-arm64
udev-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.udev-video-broker . -t $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(AMD64_TARGET)
endif
udev-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.udev-video-broker . -t $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM32V7_TARGET)
endif
udev-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.udev-video-broker . -t $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM64V8_TARGET)
endif

coap-core-build: coap-core-build-amd64 coap-core-build-arm32 coap-core-build-arm64
coap-core-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.coap-core-broker . -t $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(AMD64_TARGET)
endif
coap-core-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.coap-core-broker . -t $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM32V7_TARGET)
endif
coap-core-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.coap-core-broker . -t $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM64V8_TARGET)
endif

onvif-build: onvif-build-amd64 onvif-build-arm32 onvif-build-arm64
onvif-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.onvif-video-broker . -t $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(AMD64_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-x64
endif
onvif-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.onvif-video-broker . -t $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(ARM32V7_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm
endif
onvif-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.onvif-video-broker . -t $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(ARM64V8_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm64
endif

opcua-monitoring-build: opcua-monitoring-build-amd64 opcua-monitoring-build-arm32 opcua-monitoring-build-arm64
opcua-monitoring-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.opcua-monitoring-broker . -t $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=3.1-buster-slim --build-arg DOTNET_PUBLISH_RUNTIME=linux-x64
endif
opcua-monitoring-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.opcua-monitoring-broker . -t $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=3.1-buster-slim-$(ARM32V7_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm
endif
opcua-monitoring-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.opcua-monitoring-broker . -t $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=3.1-buster-slim-$(ARM64V8_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm64
endif

anomaly-detection-build: anomaly-detection-build-amd64 anomaly-detection-build-arm32 anomaly-detection-build-arm64
anomaly-detection-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.anomaly-detection-app . -t $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX)
endif
anomaly-detection-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.anomaly-detection-app . -t $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX)
endif
anomaly-detection-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.anomaly-detection-app . -t $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX)
endif

streaming-build: streaming-build-amd64 streaming-build-arm32 streaming-build-arm64
streaming-build-amd64:
ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.video-streaming-app . -t $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX)
endif
streaming-build-arm32:
ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.video-streaming-app . -t $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX)
endif
streaming-build-arm64:
ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.video-streaming-app . -t $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX)
endif

akri-docker-push-per-arch: controller-docker-per-arch agent-docker-per-arch udev-docker-per-arch coap-core-docker-per-arch onvif-docker-per-arch streaming-docker-per-arch opcua-monitoring-docker-per-arch anomaly-detection-docker-per-arch

controller-docker-per-arch: controller-docker-per-arch-amd64 controller-docker-per-arch-arm32 controller-docker-per-arch-arm64
controller-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/controller:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
controller-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/controller:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
controller-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/controller:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

agent-docker-per-arch: agent-docker-per-arch-amd64 agent-docker-per-arch-arm32 agent-docker-per-arch-arm64
agent-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/agent:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
agent-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/agent:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
agent-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/agent:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

onvif-docker-per-arch: onvif-docker-per-arch-amd64 onvif-docker-per-arch-arm32 onvif-docker-per-arch-arm64
onvif-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
onvif-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
onvif-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

opcua-monitoring-docker-per-arch: opcua-monitoring-docker-per-arch-amd64 opcua-monitoring-docker-per-arch-arm32 opcua-monitoring-docker-per-arch-arm64
opcua-monitoring-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
opcua-monitoring-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
opcua-monitoring-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

udev-docker-per-arch: udev-docker-per-arch-amd64 udev-docker-per-arch-arm32 udev-docker-per-arch-arm64
udev-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
udev-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
udev-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

coap-core-docker-per-arch: coap-core-docker-per-arch-amd64 coap-core-docker-per-arch-arm32 coap-core-docker-per-arch-arm64
coap-core-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
coap-core-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
coap-core-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

anomaly-detection-docker-per-arch: anomaly-detection-docker-per-arch-amd64 anomaly-detection-docker-per-arch-arm32 anomaly-detection-docker-per-arch-arm64
anomaly-detection-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
anomaly-detection-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
anomaly-detection-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

streaming-docker-per-arch: streaming-docker-per-arch-amd64 streaming-docker-per-arch-arm32 streaming-docker-per-arch-arm64
streaming-docker-per-arch-amd64:
ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
streaming-docker-per-arch-arm32:
ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
streaming-docker-per-arch-arm64:
ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

akri-docker-push-multi-arch-create: controller-docker-multi-arch-create agent-docker-multi-arch-create udev-docker-multi-arch-create coap-core-docker-multi-arch-create onvif-docker-multi-arch-create streaming-docker-multi-arch-create opcua-monitoring-docker-multi-arch-create anomaly-detection-docker-multi-arch-create

controller-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/controller:$(LABEL_PREFIX) $(PREFIX)/controller:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/controller:$(LABEL_PREFIX) $(PREFIX)/controller:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/controller:$(LABEL_PREFIX) $(PREFIX)/controller:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

agent-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/agent:$(LABEL_PREFIX) $(PREFIX)/agent:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/agent:$(LABEL_PREFIX) $(PREFIX)/agent:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/agent:$(LABEL_PREFIX) $(PREFIX)/agent:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

udev-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/udev-video-broker:$(LABEL_PREFIX) $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/udev-video-broker:$(LABEL_PREFIX) $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/udev-video-broker:$(LABEL_PREFIX) $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

coap-core-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/coap-core-broker:$(LABEL_PREFIX) $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/coap-core-broker:$(LABEL_PREFIX) $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/coap-core-broker:$(LABEL_PREFIX) $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

onvif-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX) $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX) $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX) $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

opcua-monitoring-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX) $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX) $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX) $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

anomaly-detection-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX) $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX) $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX) $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

streaming-docker-multi-arch-create:
ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/video-streaming-app:$(LABEL_PREFIX) $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(AMD64_SUFFIX)
endif
ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/video-streaming-app:$(LABEL_PREFIX) $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
endif
ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/video-streaming-app:$(LABEL_PREFIX) $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
endif

akri-docker-push-multi-arch-push: controller-docker-multi-arch-push agent-docker-multi-arch-push udev-docker-multi-arch-push onvif-docker-multi-arch-push streaming-docker-multi-arch-push opcua-monitoring-docker-multi-arch-push anomaly-detection-docker-multi-arch-push

controller-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/controller:$(LABEL_PREFIX)
agent-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/agent:$(LABEL_PREFIX)
udev-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/udev-video-broker:$(LABEL_PREFIX)
coap-core-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/coap-core-broker:$(LABEL_PREFIX)
onvif-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/onvif-video-broker:$(LABEL_PREFIX)
opcua-monitoring-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/opcua-monitoring-broker:$(LABEL_PREFIX)
anomaly-detection-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/anomaly-detection-app:$(LABEL_PREFIX)
streaming-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/video-streaming-app:$(LABEL_PREFIX)

