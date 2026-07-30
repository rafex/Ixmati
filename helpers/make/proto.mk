# helpers/make/proto.mk — generacion de codigo desde archivos .proto

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

PROTO_FILES := $(shell find $(PROTO_DIR) -name '*.proto' 2>/dev/null)

.PHONY: proto
proto:
	@echo "$(COLOR_GREEN)[PROTO] generando codigo desde .proto$(COLOR_RESET)"
	@if [ -z "$(strip $(PROTO_FILES))" ]; then \
		echo "$(COLOR_YELLOW)[PROTO] sin archivos .proto, omitiendo$(COLOR_RESET)"; \
		exit 0; \
	fi
	@mkdir -p $(OUT_DIR)
	@protoc \
		--proto_path=$(PROTO_DIR) \
		--rust_out=$(OUT_DIR) \
		--tonic_out=$(OUT_DIR) \
		$(PROTO_FILES)
	@echo "$(COLOR_GREEN)[PROTO] listo: $(OUT_DIR)$(COLOR_RESET)"

.PHONY: proto-clean
proto-clean:
	@echo "$(COLOR_YELLOW)[CLEAN] $(OUT_DIR)$(COLOR_RESET)"
	rm -rf $(OUT_DIR)
