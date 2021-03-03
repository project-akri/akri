define add_onvif_target
  akri-$(1): akri-build akri-docker-$(1)
  akri-docker-$(1): $(1)-build $(1)-docker-per-arch $(1)-docker-multi-arch-create $(1)-docker-multi-arch-push
  $(1)-build: $(1)-build-amd64 $(1)-build-arm32 $(1)-build-arm64
  $(1)-docker-per-arch: $(1)-docker-per-arch-amd64 $(1)-docker-per-arch-arm32 $(1)-docker-per-arch-arm64

  $(1)-build-amd64:
  ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(AMD64_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-x64
  endif
  $(1)-build-arm32:
  ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(ARM32V7_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm
  endif
  $(1)-build-arm64:
  ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=$(USE_OPENCV_BASE_VERSION)-$(ARM64V8_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm64
  endif

  $(1)-docker-per-arch-amd64:
  ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX)
  endif
  $(1)-docker-per-arch-arm32:
  ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
  endif
  $(1)-docker-per-arch-arm64:
  ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
  endif

  $(1)-docker-multi-arch-create:
  ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/$(2):$(LABEL_PREFIX) $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX)
  endif
  ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/$(2):$(LABEL_PREFIX) $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
  endif
  ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/$(2):$(LABEL_PREFIX) $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
  endif

  $(1)-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/$(2):$(LABEL_PREFIX)

endef

define add_opcua_target
  akri-$(1): akri-build akri-docker-$(1)
  akri-docker-$(1): $(1)-build $(1)-docker-per-arch $(1)-docker-multi-arch-create $(1)-docker-multi-arch-push
  $(1)-build: $(1)-build-amd64 $(1)-build-arm32 $(1)-build-arm64
  $(1)-docker-per-arch: $(1)-docker-per-arch-amd64 $(1)-docker-per-arch-arm32 $(1)-docker-per-arch-arm64

  $(1)-build-amd64:
  ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=3.1-buster-slim --build-arg DOTNET_PUBLISH_RUNTIME=linux-x64
  endif
  $(1)-build-arm32:
  ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=3.1-buster-slim-$(ARM32V7_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm
  endif
  $(1)-build-arm64:
  ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg OUTPUT_PLATFORM_TAG=3.1-buster-slim-$(ARM64V8_SUFFIX) --build-arg DOTNET_PUBLISH_RUNTIME=linux-arm64
  endif

  $(1)-docker-per-arch-amd64:
  ifeq (1, ${BUILD_AMD64})
	docker push $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX)
  endif
  $(1)-docker-per-arch-arm32:
  ifeq (1, ${BUILD_ARM32})
	docker push $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
  endif
  $(1)-docker-per-arch-arm64:
  ifeq (1, ${BUILD_ARM64})
	docker push $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
  endif

  $(1)-docker-multi-arch-create:
  ifeq (1, ${BUILD_AMD64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/$(2):$(LABEL_PREFIX) $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX)
  endif
  ifeq (1, ${BUILD_ARM32})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/$(2):$(LABEL_PREFIX) $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX)
  endif
  ifeq (1, ${BUILD_ARM64})
	$(ENABLE_DOCKER_MANIFEST) docker manifest create --amend $(PREFIX)/$(2):$(LABEL_PREFIX) $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX)
  endif

  $(1)-docker-multi-arch-push:
	$(ENABLE_DOCKER_MANIFEST) docker manifest push $(PREFIX)/$(2):$(LABEL_PREFIX)

endef

