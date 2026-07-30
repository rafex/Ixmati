# helpers/make/installer.mk — generacion del instalador

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

VERSION_FILE := $(REPO_ROOT)/VERSION
VERSION      := $(shell cat $(VERSION_FILE) 2>/dev/null || echo "0.0.0")

.PHONY: installer
installer:
	@echo "$(COLOR_GREEN)[INSTALLER] generando scripts/install.sh$(COLOR_RESET)"
	@mkdir -p $(DIST_DIR)
	@sed -e 's/__VERSION__/$(VERSION)/g' \
	     -e 's|__REPO__|rafex/Ixmati|g' \
	     $(REPO_ROOT)/scripts/install.sh > $(DIST_DIR)/install.sh 2>/dev/null || \
	     cp $(REPO_ROOT)/scripts/install.sh $(DIST_DIR)/install.sh
	@chmod +x $(DIST_DIR)/install.sh
	@echo "$(COLOR_GREEN)[INSTALLER] listo: $(DIST_DIR)/install.sh$(COLOR_RESET)"
