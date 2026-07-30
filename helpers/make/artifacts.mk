# helpers/make/artifacts.mk — ensamblado de dist/

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

.PHONY: dist
dist:
	@echo "$(COLOR_GREEN)[DIST] ensamblando artefactos$(COLOR_RESET)"
	@mkdir -p $(DIST_DIR)
	@cp -r $(TARGET_DIR)/release/ixmati-* $(DIST_DIR)/ 2>/dev/null || true
	@for binary in $(DIST_DIR)/ixmati-*; do \
		if [ -f "$$binary" ]; then \
			shasum -a 256 "$$binary" > "$$binary.sha256"; \
			echo "  $$(basename $$binary)"; \
		fi; \
	done
	@echo "$(COLOR_GREEN)[DIST] listo: $(DIST_DIR)$(COLOR_RESET)"
