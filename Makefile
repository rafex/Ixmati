# Root Makefile — thin, solo incluye modulos de helpers/make/
#
# Responsabilidad unica: construir artefactos.
# NUNCA invoca a just. Ver DEC-0021.
#
# Uso: make build | make proto | make containers-build | make dist | make clean

REPO_ROOT := $(shell git rev-parse --show-toplevel 2>/dev/null || pwd)
include $(REPO_ROOT)/helpers/make/common.mk

$(call check_just_forbidden)

include $(REPO_ROOT)/helpers/make/rust.mk
include $(REPO_ROOT)/helpers/make/proto.mk
include $(REPO_ROOT)/helpers/make/containers.mk
include $(REPO_ROOT)/helpers/make/artifacts.mk
