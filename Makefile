.PHONY: fmt check quick-test full-test tck-smoke tck-tier0 tck-tier1 tck-tier2 tck-tier3 tck-report pre-commit pre-push install-hooks clean help

fmt:
	@echo "format"
	@cargo fmt --all

check:
	@echo "core 0.1 check"
	@bash scripts/check.sh

quick-test:
	@echo "core 0.1 quick test"
	@bash scripts/workspace_quick_test.sh

full-test:
	@echo "full historical workspace verification"
	@bash scripts/workspace_full_test.sh

test: full-test

tck-smoke:
	@echo "manual TCK smoke gate"
	@bash scripts/tck_smoke_gate.sh

tck-tier0:
	@echo "manual TCK Tier-0"
	@bash scripts/tck_tier_gate.sh tier0

tck-tier1:
	@echo "manual TCK Tier-1"
	@bash scripts/tck_tier_gate.sh tier1

tck-tier2:
	@echo "manual TCK Tier-2"
	@bash scripts/tck_tier_gate.sh tier2

tck-tier3:
	@echo "manual TCK Tier-3"
	@bash scripts/tck_tier_gate.sh tier3

tck-report:
	@echo "TCK failure cluster report"
	@bash scripts/tck_failure_cluster.sh artifacts/tck/tier3-full.log artifacts/tck/tier3-cluster.md

pre-commit:
	@echo "pre-commit core check"
	@bash scripts/check.sh

pre-push:
	@echo "pre-push core check"
	@bash scripts/check.sh

install-hooks:
	@echo "installing git hooks"
	@cp scripts/git-hooks/pre-commit .git/hooks/pre-commit
	@cp scripts/git-hooks/pre-push .git/hooks/pre-push
	@chmod +x .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-push
	@echo "git hooks installed"

clean:
	@echo "cleaning build artifacts"
	@cargo clean

help:
	@echo "NervusDB development commands:"
	@echo "  make check      - Run the default core 0.1 check"
	@echo "  make quick-test - Run the core 0.1 Mini-Cypher test"
	@echo "  make full-test  - Run full historical workspace verification manually"
	@echo "  make test       - Alias for make full-test"
	@echo "  make pre-commit - Run the fast core pre-commit check"
	@echo "  make pre-push   - Run the fast core pre-push check"
	@echo "  make tck-tier0  - Manual TCK Tier-0"
	@echo "  make tck-tier1  - Manual TCK Tier-1"
	@echo "  make tck-tier2  - Manual TCK Tier-2"
	@echo "  make tck-tier3  - Manual TCK Tier-3"
	@echo "  make install-hooks - Install fast core hooks"
	@echo "  make clean      - Clean build artifacts"
