# helpers/make/common.mk — variables compartidas y helpers del sistema de build

# ── deteccion de SO y arquitectura ──
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)
IS_MACOS := $(filter Darwin,$(UNAME_S))
IS_LINUX := $(filter Linux,$(UNAME_S))
IS_ARM  := $(filter arm64 aarch64,$(UNAME_M))
IS_X86  := $(filter x86_64,$(UNAME_M))

# ── paths ──
REPO_ROOT   := $(shell git rev-parse --show-toplevel 2>/dev/null || pwd)
DIST_DIR    := $(REPO_ROOT)/dist
TARGET_DIR  := $(REPO_ROOT)/target
PROTO_DIR   := $(REPO_ROOT)/proto
CONFIG_DIR  := $(REPO_ROOT)/config
OUT_DIR     := $(TARGET_DIR)/generated

# ── rust ──
CARGO       := cargo
ifeq ($(IS_ARM),arm64 aarch64)
	RUST_TARGET := aarch64-unknown-linux-musl
else
	RUST_TARGET := x86_64-unknown-linux-musl
endif
RUSTFLAGS   := -D warnings
CARGO_FLAGS := --workspace

# ── colores ──
COLOR_RESET  := \033[0m
COLOR_BOLD   := \033[1m
COLOR_GREEN  := \033[32m
COLOR_YELLOW := \033[33m
COLOR_RED    := \033[31m
COLOR_BLUE   := \033[34m

# ── guard: make no puede llamar a just ──
define check_just_forbidden
	@if grep -rq 'just ' $(MAKEFILE_LIST) $(REPO_ROOT)/helpers/make/ ; then \
		echo "$(COLOR_RED)[ERROR] Make no puede invocar a just. Usa helpers/shell/ en su lugar.$(COLOR_RESET)"; \
		echo "$(COLOR_RED)       Ver DEC-0021 en spec-native/DECISIONS.md$(COLOR_RESET)"; \
		exit 1; \
	fi
endef

.PHONY: help
help:
	@echo "$(COLOR_BLUE)Ixmati — Build System (make)$(COLOR_RESET)"
	@echo ""
	@echo "$(COLOR_BOLD)Targets:$(COLOR_RESET)"
	@echo "  build          Compila en modo debug"
	@echo "  build-release  Compila en modo release"
	@echo "  build-musl     Compila con target musl (static binary)"
	@echo "  proto          Genera codigo desde archivos .proto"
	@echo "  docker         Construye imagenes Docker"
	@echo "  dist           Ensambla artefactos de distribucion en dist/"
	@echo "  doc            Genera documentacion (cargo doc)"
	@echo "  clean          Limpia target/ y dist/"
	@echo ""
	@echo "Para tareas de desarrollo usa just (test, fmt, lint, hooks, docs, etc.)"
