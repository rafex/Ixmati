# helpers/make/ci.mk — CI local (independiente de GitHub Actions)
#
# Uso:
#   make ci-pr     → gates rapidos (fmt, clippy, tests) — no requiere podman
#   make ci-main   → CI completo (container compile, dist, validacion) — requiere podman
#   make ci        → alias de ci-main

include $(dir $(lastword $(MAKEFILE_LIST)))/common.mk

.PHONY: ci-pr
ci-pr:
	@echo "$(COLOR_BLUE)[CI] PR gates...$(COLOR_RESET)"
	@$(CARGO) fmt --all -- --check
	@$(CARGO) clippy --all-targets --all-features --workspace -- -D warnings
	@$(CARGO) test --lib --workspace
	@$(CARGO) test -p ixmati-integration
	@echo "$(COLOR_GREEN)[CI] PR OK$(COLOR_RESET)"

.PHONY: ci-main
ci-main: ci-pr
	@echo "$(COLOR_BLUE)[CI] main gates (container linux/amd64)...$(COLOR_RESET)"
	@make containers-builder
	@make containers-compile
	@make dist
	@make dist-checksums
	@make dist-validate
	@echo "$(COLOR_GREEN)[CI] main OK$(COLOR_RESET)"

.PHONY: ci-allinone
ci-allinone: ci-pr containers-builder containers-allinone
	@echo "$(COLOR_GREEN)[CI] allinone image built$(COLOR_RESET)"
	@$(PODMAN) images | grep ixmati-allinone

.PHONY: ci
ci: ci-main
