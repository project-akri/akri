
# Create set of targets for rust builds
define add_rust_targets
  akri-$(1): akri-build akri-docker-$(1)
  akri-docker-$(1): $(1)-build $(1)-docker-per-arch $(1)-docker-multi-arch-create $(1)-docker-multi-arch-push
  $(1)-build: $(1)-build-amd64 $(1)-build-arm32 $(1)-build-arm64
  $(1)-docker-per-arch: $(1)-docker-per-arch-amd64 $(1)-docker-per-arch-arm32 $(1)-docker-per-arch-arm64

  $(1)-build-amd64:
  ifeq (1, ${BUILD_AMD64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(AMD64_SUFFIX) --build-arg PLATFORM=$(AMD64_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(AMD64_TARGET)
  endif
  $(1)-build-arm32:
  ifeq (1, ${BUILD_ARM32})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM32V7_SUFFIX) --build-arg PLATFORM=$(ARM32V7_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM32V7_TARGET)
  endif
  $(1)-build-arm64:
  ifeq (1, ${BUILD_ARM64})
	docker build $(CACHE_OPTION) -f $(DOCKERFILE_DIR)/Dockerfile.$(2) . -t $(PREFIX)/$(2):$(LABEL_PREFIX)-$(ARM64V8_SUFFIX) --build-arg PLATFORM=$(ARM64V8_SUFFIX) --build-arg CROSS_BUILD_TARGET=$(ARM64V8_TARGET)
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
