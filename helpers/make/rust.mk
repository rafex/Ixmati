# helpers/make/rust.mk — targets de compilacion Rust

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

.PHONY: build
build:
	@echo "$(COLOR_GREEN)[BUILD] debug$(COLOR_RESET)"
	$(CARGO) build $(CARGO_FLAGS)

.PHONY: build-release
build-release:
	@echo "$(COLOR_GREEN)[BUILD] release$(COLOR_RESET)"
	$(CARGO) build $(CARGO_FLAGS) --release

.PHONY: build-musl
build-musl:
	@echo "$(COLOR_GREEN)[BUILD] musl ($(RUST_TARGET))$(COLOR_RESET)"
	$(CARGO) build $(CARGO_FLAGS) --release --target $(RUST_TARGET)

.PHONY: doc
doc:
	@echo "$(COLOR_GREEN)[DOC] cargo doc$(COLOR_RESET)"
	$(CARGO) doc $(CARGO_FLAGS) --no-deps --document-private-items

.PHONY: clean
clean:
	@echo "$(COLOR_YELLOW)[CLEAN] target/ dist/$(COLOR_RESET)"
	rm -rf $(TARGET_DIR) $(DIST_DIR) $(OUT_DIR)
