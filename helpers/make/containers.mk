# helpers/make/containers.mk — construccion de imagenes de contenedor con podman

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

PODMAN          ?= podman
PODMAN_COMPOSE  ?= podman compose
IMAGE_PREFIX    ?= localhost/ixmati
IMAGE_TAG       ?= local

CONTAINER_DIR   := $(REPO_ROOT)/containers
SERVICES        := api writer projector supervisor reconciler cache-server
INFRA_IMAGES    := mosquitto litestream

# ── builder compartido (debe construirse primero) ──
.PHONY: containers-builder
containers-builder:
	@echo "$(COLOR_GREEN)[CONTAINERS] builder compartido$(COLOR_RESET)"
	$(PODMAN) build \
		-f $(CONTAINER_DIR)/base/Containerfile \
		-t $(IMAGE_PREFIX)-builder:$(IMAGE_TAG) \
		$(REPO_ROOT)

# ── servicios ──
.PHONY: containers-services
containers-services: containers-builder
	@for svc in $(SERVICES); do \
		echo "$(COLOR_GREEN)[CONTAINERS] $$svc$(COLOR_RESET)"; \
		$(PODMAN) build \
			-f $(CONTAINER_DIR)/$$svc/Containerfile \
			-t $(IMAGE_PREFIX)-$$svc:$(IMAGE_TAG) \
			$(REPO_ROOT) ; \
	done

# ── imagenes de infra ──
.PHONY: containers-infra
containers-infra:
	@echo "$(COLOR_GREEN)[CONTAINERS] mosquitto$(COLOR_RESET)"
	$(PODMAN) build \
		-f $(CONTAINER_DIR)/mosquitto/Containerfile \
		-t $(IMAGE_PREFIX)-mosquitto:$(IMAGE_TAG) \
		$(CONTAINER_DIR)/mosquitto
	@echo "$(COLOR_GREEN)[CONTAINERS] litestream$(COLOR_RESET)"
	$(PODMAN) build \
		-f $(CONTAINER_DIR)/litestream/Containerfile \
		-t $(IMAGE_PREFIX)-litestream:$(IMAGE_TAG) \
		$(CONTAINER_DIR)/litestream

# ── compilar binarios dentro del builder container ──
.PHONY: containers-compile
containers-compile:
	@echo "$(COLOR_GREEN)[CONTAINERS] extrayendo binarios linux/amd64 del builder...$(COLOR_RESET)"
	@mkdir -p $(TARGET_DIR)/release
	@CONTAINER_ID=$$($(PODMAN) create $(IMAGE_PREFIX)-builder:$(IMAGE_TAG)); \
	echo "  container=$$CONTAINER_ID"; \
	for binary in ixmati-api ixmati-writer ixmati-projector ixmati-supervisor ixmati-reconciler ixmati-cache-server; do \
		$(PODMAN) cp "$$CONTAINER_ID:/usr/local/bin/$$binary" "$(TARGET_DIR)/release/$$binary" 2>/dev/null && \
			ls -lh "$(TARGET_DIR)/release/$$binary" || \
			echo "  $(COLOR_YELLOW)$$binary fallo$(COLOR_RESET)"; \
	done; \
	$(PODMAN) rm "$$CONTAINER_ID" 2>/dev/null; \
	echo "$(COLOR_GREEN)[CONTAINERS] binarios amd64 listos$(COLOR_RESET)"

# ── imagen all-in-one (Mosquitto + API + Writer + Projector en un container) ──
.PHONY: containers-allinone
containers-allinone: containers-builder
	@echo "$(COLOR_GREEN)[CONTAINERS] all-in-one (Mosquitto + API + Writer + Projector)$(COLOR_RESET)"
	$(PODMAN) build --network=host \
		-f $(CONTAINER_DIR)/allinone/Containerfile \
		-t $(IMAGE_PREFIX)-allinone:$(IMAGE_TAG) \
		$(REPO_ROOT)

# ── todas las imagenes ──
.PHONY: containers-build
containers-build: containers-builder containers-services containers-infra

# ── compose ──
.PHONY: containers-up
containers-up:
	@echo "$(COLOR_GREEN)[CONTAINERS] levantando $(COMPOSE_FILE)$(COLOR_RESET)"
	$(PODMAN_COMPOSE) -f $(CONTAINER_DIR)/compose/$(COMPOSE_FILE) up -d

.PHONY: containers-down
containers-down:
	@echo "$(COLOR_YELLOW)[CONTAINERS] deteniendo $(COMPOSE_FILE)$(COLOR_RESET)"
	$(PODMAN_COMPOSE) -f $(CONTAINER_DIR)/compose/$(COMPOSE_FILE) down

# ── limpieza ──
.PHONY: containers-clean
containers-clean:
	@echo "$(COLOR_YELLOW)[CLEAN] imagenes ixmati$(COLOR_RESET)"
	@for img in builder $(SERVICES) $(INFRA_IMAGES); do \
		$(PODMAN) rmi $(IMAGE_PREFIX)-$$img:$(IMAGE_TAG) 2>/dev/null || true; \
	done
