.PHONY: fmt check test quick-test pre-commit install-hooks clean

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
	@echo "  make pre-commit    - Run all pre-commit checks"
	@echo "  make install-hooks - Install git hooks"
	@echo "  make clean         - Clean build artifacts"
