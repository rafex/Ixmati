# helpers/make/proto.mk — validación reproducible del contrato .proto

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

PROTO_FILES := $(shell find $(PROTO_DIR) -name '*.proto' 2>/dev/null)

.PHONY: proto
proto:
	@echo "$(COLOR_GREEN)[PROTO] validando archivos .proto$(COLOR_RESET)"
	@if [ -z "$(strip $(PROTO_FILES))" ]; then \
		echo "$(COLOR_YELLOW)[PROTO] sin archivos .proto, omitiendo$(COLOR_RESET)"; \
		exit 0; \
	fi
	@descriptor="$$(mktemp)"; \
		trap 'rm -f "$$descriptor"' EXIT; \
		protoc --proto_path=$(PROTO_DIR) --include_imports \
			--descriptor_set_out="$$descriptor" $(PROTO_FILES); \
		echo "$(COLOR_GREEN)[PROTO] contrato valido$(COLOR_RESET)"

.PHONY: proto-clean
proto-clean:
	@echo "$(COLOR_YELLOW)[CLEAN] $(OUT_DIR)$(COLOR_RESET)"
	rm -rf $(OUT_DIR)
