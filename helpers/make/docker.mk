# helpers/make/docker.mk — construccion de imagenes Docker

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

DOCKER_REGISTRY ?= ixmati
DOCKER_TAG      ?= latest
DOCKER_CRATES   := ixmati-api ixmati-writer ixmati-projector ixmati-reconciler ixmati-supervisor

.PHONY: docker
docker: docker-build

.PHONY: docker-build
docker-build:
	@echo "$(COLOR_GREEN)[DOCKER] construyendo imagenes$(COLOR_RESET)"
	@for crate in $(DOCKER_CRATES); do \
		echo "  $$crate"; \
		docker build \
			-f docker/Dockerfile.$$crate \
			-t $(DOCKER_REGISTRY)/$$crate:$(DOCKER_TAG) \
			. ; \
	done
	@echo "$(COLOR_GREEN)[DOCKER] listo$(COLOR_RESET)"

.PHONY: docker-push
docker-push:
	@echo "$(COLOR_GREEN)[DOCKER] push al registry$(COLOR_RESET)"
	@for crate in $(DOCKER_CRATES); do \
		docker push $(DOCKER_REGISTRY)/$$crate:$(DOCKER_TAG) ; \
	done
