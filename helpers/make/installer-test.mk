# helpers/make/installer-test.mk — operativa del instalador nativo en
# contenedor Debian (systemd real como PID 1, via podman --privileged)
#
# Ciclo de vida granular para depuracion manual:
#   make installer-test-image      → build de la imagen Debian+systemd
#   make installer-test-up         → levanta el contenedor privilegiado
#   make installer-test-install    → copia dist/ y corre install.sh dentro
#   make installer-test-status     → systemctl is-active de los 5 servicios
#   make installer-test-logs       → journalctl de un servicio (SVC=ixmati-api)
#   make installer-test-shell      → shell interactiva dentro del contenedor
#   make installer-test-uninstall  → corre install.sh --uninstall --purge
#   make installer-test-down       → detiene y elimina el contenedor
#   make installer-test-clean      → down + elimina la imagen de test
#
# Flujo completo automatizado (instala, verifica, reinstala, desinstala):
#   make installer-test

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

PODMAN               ?= podman
IMAGE_PREFIX         ?= localhost/ixmati
IMAGE_TAG            ?= local

INSTALLER_TEST_IMAGE := ixmati-installer-test
INSTALLER_TEST_NAME  := ixmati-installer-test
VERSION_FILE         := $(REPO_ROOT)/VERSION
VERSION              := $(shell cat $(VERSION_FILE) 2>/dev/null || echo "0.0.0")
TARBALL              := $(DIST_DIR)/ixmati-$(VERSION)-linux-amd64.tar.gz
DIST_DIRNAME         := ixmati-$(VERSION)-linux-amd64
SVC                  ?= ixmati-api

.PHONY: installer-test-image
installer-test-image:
	@echo "$(COLOR_GREEN)[INSTALLER-TEST] build imagen Debian + systemd$(COLOR_RESET)"
	$(PODMAN) build -t $(INSTALLER_TEST_IMAGE) $(REPO_ROOT)/containers/installer-test

.PHONY: installer-test-up
installer-test-up:
	@echo "$(COLOR_GREEN)[INSTALLER-TEST] levantando contenedor privilegiado$(COLOR_RESET)"
	@$(PODMAN) rm -f $(INSTALLER_TEST_NAME) >/dev/null 2>&1 || true
	$(PODMAN) run -d --name $(INSTALLER_TEST_NAME) --privileged \
		-v /sys/fs/cgroup:/sys/fs/cgroup:rw \
		$(INSTALLER_TEST_IMAGE)
	@echo "$(COLOR_GREEN)[INSTALLER-TEST] esperando systemd...$(COLOR_RESET)"
	@for i in $$(seq 1 30); do \
		state=$$($(PODMAN) exec $(INSTALLER_TEST_NAME) systemctl is-system-running 2>/dev/null || true); \
		if [ "$$state" = "running" ] || [ "$$state" = "degraded" ]; then \
			echo "  $(COLOR_GREEN)✓$(COLOR_RESET) systemd listo ($$state)"; exit 0; \
		fi; \
		sleep 1; \
	done; \
	echo "$(COLOR_RED)systemd no llego a estado operativo$(COLOR_RESET)"; exit 1

.PHONY: installer-test-install
installer-test-install:
	@test -f $(TARBALL) || { echo "$(COLOR_RED)tarball no encontrado: $(TARBALL) (ejecuta make dist)$(COLOR_RESET)"; exit 1; }
	@echo "$(COLOR_GREEN)[INSTALLER-TEST] instalando dentro del contenedor$(COLOR_RESET)"
	$(PODMAN) cp $(TARBALL) $(INSTALLER_TEST_NAME):/root/$(notdir $(TARBALL))
	$(PODMAN) exec $(INSTALLER_TEST_NAME) bash -c "cd /root && tar xzf $(notdir $(TARBALL))"
	$(PODMAN) exec $(INSTALLER_TEST_NAME) bash -c "cd /root/$(DIST_DIRNAME) && ./install.sh"

.PHONY: installer-test-status
installer-test-status:
	@echo "$(COLOR_BLUE)[INSTALLER-TEST] estado de servicios$(COLOR_RESET)"
	@for svc in mosquitto ixmati-cache-server ixmati-writer@default ixmati-api ixmati-projector; do \
		status=$$($(PODMAN) exec $(INSTALLER_TEST_NAME) systemctl is-active $$svc 2>/dev/null || echo inactive); \
		if [ "$$status" = "active" ]; then \
			echo "  $(COLOR_GREEN)✓$(COLOR_RESET) $$svc → $$status"; \
		else \
			echo "  $(COLOR_RED)✗$(COLOR_RESET) $$svc → $$status"; \
		fi; \
	done
	@$(PODMAN) exec $(INSTALLER_TEST_NAME) curl -sS http://localhost:30000/health && echo "" || true

.PHONY: installer-test-logs
installer-test-logs:
	$(PODMAN) exec $(INSTALLER_TEST_NAME) journalctl -u $(SVC) --no-pager -n 80

.PHONY: installer-test-shell
installer-test-shell:
	$(PODMAN) exec -it $(INSTALLER_TEST_NAME) bash

.PHONY: installer-test-uninstall
installer-test-uninstall:
	@echo "$(COLOR_YELLOW)[INSTALLER-TEST] desinstalando (--purge)$(COLOR_RESET)"
	$(PODMAN) exec $(INSTALLER_TEST_NAME) bash -c "cd /root/$(DIST_DIRNAME) && ./install.sh --uninstall --purge"

.PHONY: installer-test-down
installer-test-down:
	@echo "$(COLOR_YELLOW)[INSTALLER-TEST] deteniendo contenedor$(COLOR_RESET)"
	@$(PODMAN) rm -f $(INSTALLER_TEST_NAME) >/dev/null 2>&1 || true

.PHONY: installer-test-clean
installer-test-clean: installer-test-down
	@echo "$(COLOR_YELLOW)[INSTALLER-TEST] eliminando imagen de test$(COLOR_RESET)"
	@$(PODMAN) rmi $(INSTALLER_TEST_IMAGE) 2>/dev/null || true

# ── flujo completo automatizado (instala, verifica round-trip, reinstala,
#    verifica idempotencia, desinstala con purge) ──
.PHONY: installer-test
installer-test:
	@$(REPO_ROOT)/helpers/shell/test_installer_debian.sh
