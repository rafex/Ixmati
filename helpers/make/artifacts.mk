# helpers/make/artifacts.mk — ensamblado de dist/
#
# Genera:
#   make dist           → dist/ixmati-<VERSION>-linux-<ARCH>.tar.gz
#   make dist-checksums → dist/*.sha256
#   make dist-clean     → rm -rf dist/

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

VERSION_FILE := $(REPO_ROOT)/VERSION
VERSION      := $(shell cat $(VERSION_FILE) 2>/dev/null || echo "0.0.0")
DIST_ARCH    := amd64
DIST_NAME    := ixmati-$(VERSION)-linux-$(DIST_ARCH)
DIST_DIR     := $(REPO_ROOT)/dist
TARGET_DIR   := $(REPO_ROOT)/target
SCRIPTS_DIR  := $(REPO_ROOT)/scripts
BINARIES     := ixmati-cache-server ixmati-api ixmati-writer ixmati-projector ixmati-supervisor ixmati-reconciler ixmati-store-migrate

.PHONY: dist
# A linux/amd64 distribution must be built by the reproducible Podman builder,
# never by the host toolchain (which may produce Mach-O/arm64 on macOS) and
# never by reusing stale files from target/release.
dist: containers-builder containers-compile installer
	@echo "$(COLOR_GREEN)[DIST] ensamblando $(DIST_NAME).tar.gz$(COLOR_RESET)"
	@mkdir -p $(DIST_DIR)/$(DIST_NAME)/bin
	@mkdir -p $(DIST_DIR)/$(DIST_NAME)/config/mosquitto
	@mkdir -p $(DIST_DIR)/$(DIST_NAME)/systemd
	@for binary in $(BINARIES); do \
		if [ -f "$(TARGET_DIR)/release/$$binary" ]; then \
			cp "$(TARGET_DIR)/release/$$binary" $(DIST_DIR)/$(DIST_NAME)/bin/ ; \
			echo "  $$binary"; \
		else \
			echo "  $(COLOR_YELLOW)$$binary no encontrado, omitiendo$(COLOR_RESET)"; \
		fi; \
	done
	@cp $(REPO_ROOT)/config/stores.toml $(DIST_DIR)/$(DIST_NAME)/config/ 2>/dev/null || true
	@cp $(REPO_ROOT)/config/projections.toml $(DIST_DIR)/$(DIST_NAME)/config/ 2>/dev/null || true
	@cp $(REPO_ROOT)/config/stores.example.toml $(DIST_DIR)/$(DIST_NAME)/config/ 2>/dev/null || true
	@cp $(REPO_ROOT)/config/projections.example.toml $(DIST_DIR)/$(DIST_NAME)/config/ 2>/dev/null || true
	@cp $(REPO_ROOT)/config/mosquitto/mosquitto.conf $(DIST_DIR)/$(DIST_NAME)/config/mosquitto/ 2>/dev/null || true
	@cp $(REPO_ROOT)/config/litestream.yml $(DIST_DIR)/$(DIST_NAME)/config/ 2>/dev/null || true
	@for unit in $(REPO_ROOT)/systemd/ixmati-*.service; do \
		cp "$$unit" $(DIST_DIR)/$(DIST_NAME)/systemd/ 2>/dev/null || true; \
	done
	@cp $(REPO_ROOT)/CHANGELOG.md $(DIST_DIR)/$(DIST_NAME)/ 2>/dev/null || true
	@cp $(REPO_ROOT)/VERSION $(DIST_DIR)/$(DIST_NAME)/ 2>/dev/null || true
	@cp $(DIST_DIR)/install.sh $(DIST_DIR)/$(DIST_NAME)/ 2>/dev/null || true
	@cp $(REPO_ROOT)/helpers/python/installer.py $(DIST_DIR)/$(DIST_NAME)/ 2>/dev/null || true
	@echo "$(COLOR_GREEN)[DIST] empaquetando$(COLOR_RESET)"
	@cd $(DIST_DIR) && tar czf $(DIST_NAME).tar.gz $(DIST_NAME)/
	@rm -rf $(DIST_DIR)/$(DIST_NAME)
	@echo "$(COLOR_GREEN)[DIST] listo: $(DIST_DIR)/$(DIST_NAME).tar.gz$(COLOR_RESET)"

.PHONY: dist-checksums
dist-checksums:
	@echo "$(COLOR_GREEN)[DIST] generando checksums$(COLOR_RESET)"
	@cd $(DIST_DIR) && for f in *.tar.gz; do \
		shasum -a 256 "$$f" > "$$f.sha256"; \
		echo "  $$f.sha256"; \
	done

.PHONY: dist-clean
dist-clean:
	@echo "$(COLOR_YELLOW)[CLEAN] dist/$(COLOR_RESET)"
	rm -rf $(DIST_DIR)
