.PHONY: fmt check test quick-test tck-smoke tck-tier0 tck-tier1 tck-tier2 tck-tier3 tck-report pre-commit install-hooks clean

# Format code
fmt:
	@echo "📝 Formatting code..."
	@cargo fmt --all

# Run clippy linter
check:
	@echo "🔍 Running clippy..."
	@cargo clippy --workspace --all-targets -- -W warnings

# Run quick library tests
quick-test:
	@echo "⚡ Running quick tests..."
	@cargo test --lib --workspace --no-fail-fast

# Run full test suite
test:
	@echo "🧪 Running full test suite..."
	@cargo test --workspace

# Run legacy TCK smoke gate
tck-smoke:
	@echo "🧭 Running TCK smoke gate..."
	@bash scripts/tck_smoke_gate.sh

# Tiered TCK gates
tck-tier0:
	@echo "🧭 Running TCK Tier-0..."
	@bash scripts/tck_tier_gate.sh tier0

tck-tier1:
	@echo "🧭 Running TCK Tier-1..."
	@bash scripts/tck_tier_gate.sh tier1

tck-tier2:
	@echo "🧭 Running TCK Tier-2..."
	@bash scripts/tck_tier_gate.sh tier2

tck-tier3:
	@echo "🧭 Running TCK Tier-3..."
	@bash scripts/tck_tier_gate.sh tier3

tck-report:
	@echo "📊 Building TCK failure cluster report..."
	@bash scripts/tck_failure_cluster.sh artifacts/tck/tier3-full.log artifacts/tck/tier3-cluster.md

# Pre-commit check (fmt + clippy + quick tests)
pre-commit: fmt check quick-test
	@echo "✅ Pre-commit checks passed"

# Install git hooks
install-hooks:
	@echo "📦 Installing git hooks..."
	@cp scripts/git-hooks/pre-commit .git/hooks/pre-commit
	@cp scripts/git-hooks/pre-push .git/hooks/pre-push
	@chmod +x .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-push
	@echo "✅ Git hooks installed! Run 'make pre-commit' to test them."

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	@cargo clean

# Show help
help:
	@echo "NervusDB Development Commands:"
	@echo "  make fmt           - Format code with rustfmt"
	@echo "  make check         - Run clippy linter"
	@echo "  make quick-test    - Run quick library tests"
	@echo "  make test          - Run full test suite"
	@echo "  make tck-tier0     - Run TCK Tier-0 (smoke)"
	@echo "  make tck-tier1     - Run TCK Tier-1 (clauses whitelist)"
	@echo "  make tck-tier2     - Run TCK Tier-2 (expressions whitelist)"
	@echo "  make tck-tier3     - Run TCK Tier-3 (full, may fail)"
	@echo "  make tck-report    - Build TCK failure cluster report"
	@echo "  make pre-commit    - Run all pre-commit checks"
	@echo "  make install-hooks - Install git hooks"
	@echo "  make clean         - Clean build artifacts"
