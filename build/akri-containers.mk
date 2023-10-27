#
#
# AKRI: make and push the images for akri:
#
#    To make all platforms: `make akri`
#    To make specific platforms: `BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=1 make akri`
#    To make single component: `make akri-[controller|agent|udev|onvif|streaming|opcua-monitoring|anomaly-detection|webhook-configuration|debug-echo-discovery|udev-discovery|onvif-discovery|opcua-discovery]`
#    To make specific platforms: `BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=1 make akri-[controller|agent|udev|onvif|streaming|opcua-monitoring|anomaly-detection|webhook-configuration|debug-echo-discovery|udev-discovery|onvif-discovery|opcua-discovery]`
#	 To make an agent with embedded discovery handlers (on all platforms): `FULL_AGENT_EXECUTABLE_NAME=agent AGENT_FEATURES="agent-full onvif-feat opcua-feat udev-feat" make akri-agent` 
#	 To make a slim agent without any embedded discovery handlers: `BUILD_SLIM_AGENT=1 make akri-agent` 
# 	 To make a slim and full Agent, with full agent executable renamed agent-full: `AGENT_FEATURES="agent-full onvif-feat opcua-feat udev-feat" BUILD_SLIM_AGENT=1 make akri-agent` 
#
.PHONY: akri
akri: akri-agent akri-agent-full akri-controller akri-webhook-configuration akri-debug-echo-discovery-handler akri-onvif-discovery-handler akri-opcua-discovery-handler akri-udev-discovery-handler

akri-%:
	docker buildx build $(COMMON_DOCKER_BUILD_ARGS) --build-arg AKRI_COMPONENT=$* --tag "$(PREFIX)/$(subst -handler,,$*):$(LABEL_PREFIX)" --build-arg EXTRA_CARGO_ARGS="$(if $(BUILD_RELEASE_FLAG), --release)" --file $(DOCKERFILE_DIR)/Dockerfile.rust . 

.PHONY: akri-agent-full
akri-agent-full:
ifneq (,$(strip $(AGENT_FEATURES)))
	docker buildx build $(COMMON_DOCKER_BUILD_ARGS) --build-arg AKRI_COMPONENT=agent --build-arg EXTRA_CARGO_ARGS="$(if $(BUILD_RELEASE_FLAG), --release) -F agent-full,$(subst $(space),$(comma),$(AGENT_FEATURES))" --tag "$(PREFIX)/agent-full:$(LABEL_PREFIX)" --file $(DOCKERFILE_DIR).rust .
endif

