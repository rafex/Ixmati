# helpers/make/dist-validate.mk — validacion del artefacto dist/
#
# Uso: make dist-validate

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

VERSION_FILE := $(REPO_ROOT)/VERSION
VERSION      := $(shell cat $(VERSION_FILE) 2>/dev/null || echo "0.0.0")
DIST_ARCH    := amd64
DIST_NAME    := ixmati-$(VERSION)-linux-$(DIST_ARCH)

EXPECTED_BINS    := ixmati-cache-server ixmati-api ixmati-writer ixmati-projector ixmati-supervisor ixmati-reconciler ixmati-store-migrate
EXPECTED_CONFIG  := stores.toml projections.toml
EXPECTED_SYSTEMD := ixmati-cache-server.service ixmati-api.service ixmati-writer@.service ixmati-projector.service ixmati-litestream-file.service ixmati-litestream-s3.service

.PHONY: dist-validate
dist-validate:
	@FAIL=0; \
	TARBALL="$(DIST_DIR)/$(DIST_NAME).tar.gz"; \
	echo "$(COLOR_BLUE)[VALIDATE] $(DIST_NAME).tar.gz$(COLOR_RESET)"; \
	\
	if [ ! -f "$$TARBALL" ]; then \
		echo "  $(COLOR_RED)✗$(COLOR_RESET) tarball no encontrado: $$TARBALL"; exit 1; \
	fi; \
	size=$$(du -h "$$TARBALL" | cut -f1); \
	echo "  $(COLOR_GREEN)✓$(COLOR_RESET) tarball existe ($$size)"; \
	\
	cd "$(DIST_DIR)" && shasum -a 256 -c "$(DIST_NAME).tar.gz.sha256" 2>/dev/null && \
		echo "  $(COLOR_GREEN)✓$(COLOR_RESET) checksum SHA256 verificado" || \
		{ echo "  $(COLOR_RED)✗$(COLOR_RESET) checksum no coincide"; FAIL=$$((FAIL+1)); }; \
	\
	for binary in $(EXPECTED_BINS); do \
		if tar tzf "$$TARBALL" | grep -q "bin/$$binary"; then \
			binary_size=$$(tar xzf "$$TARBALL" -O "$(DIST_NAME)/bin/$$binary" 2>/dev/null | wc -c); \
			if [ "$$binary_size" -gt 50000 ]; then \
				binary_type=$$(tar xzf "$$TARBALL" -O "$(DIST_NAME)/bin/$$binary" 2>/dev/null | file - 2>/dev/null || true); \
				case "$$binary_type" in \
					*"ELF 64-bit"*"x86-64"*) \
						echo "  $(COLOR_GREEN)✓$(COLOR_RESET) bin/$$binary ($${binary_size} bytes, linux/amd64)"; \
						;; \
					*) \
						echo "  $(COLOR_RED)✗$(COLOR_RESET) bin/$$binary no es ELF linux/amd64: $$binary_type"; FAIL=$$((FAIL+1)); \
						;; \
				 esac; \
			else \
				echo "  $(COLOR_RED)✗$(COLOR_RESET) bin/$$binary corrupto ($${binary_size} bytes)"; FAIL=$$((FAIL+1)); \
			fi; \
		else \
			echo "  $(COLOR_RED)✗$(COLOR_RESET) bin/$$binary faltante"; FAIL=$$((FAIL+1)); \
		fi; \
	done; \
	\
	for cfg in $(EXPECTED_CONFIG); do \
		if tar tzf "$$TARBALL" | grep -q "config/$$cfg"; then \
			echo "  $(COLOR_GREEN)✓$(COLOR_RESET) config/$$cfg"; \
		else \
			echo "  $(COLOR_RED)✗$(COLOR_RESET) config/$$cfg faltante"; FAIL=$$((FAIL+1)); \
		fi; \
	done; \
	\
	if tar tzf "$$TARBALL" | grep -q "mosquitto.conf"; then \
		echo "  $(COLOR_GREEN)✓$(COLOR_RESET) config/mosquitto/mosquitto.conf"; \
	else \
		echo "  $(COLOR_RED)✗$(COLOR_RESET) config/mosquitto/mosquitto.conf faltante"; FAIL=$$((FAIL+1)); \
	fi; \
	\
	for unit in $(EXPECTED_SYSTEMD); do \
		if tar tzf "$$TARBALL" | grep -q "systemd/$$unit"; then \
			echo "  $(COLOR_GREEN)✓$(COLOR_RESET) systemd/$$unit"; \
		else \
			echo "  $(COLOR_RED)✗$(COLOR_RESET) systemd/$$unit faltante"; FAIL=$$((FAIL+1)); \
		fi; \
	done; \
	\
	for extra in install.sh VERSION CHANGELOG.md; do \
		if tar tzf "$$TARBALL" | grep -q "$$extra"; then \
			echo "  $(COLOR_GREEN)✓$(COLOR_RESET) $$extra"; \
		else \
			echo "  $(COLOR_RED)✗$(COLOR_RESET) $$extra faltante"; FAIL=$$((FAIL+1)); \
		fi; \
	done; \
	\
	echo ""; \
	if [ "$$FAIL" -gt 0 ]; then \
		echo "$(COLOR_RED)[VALIDATE] ✗ $$FAIL errores$(COLOR_RESET)"; exit 1; \
	else \
		echo "$(COLOR_GREEN)[VALIDATE] ✓ artefacto valido$(COLOR_RESET)"; \
	fi
