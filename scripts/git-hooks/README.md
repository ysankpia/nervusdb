# Git Hooks for NervusDB

This directory contains git hooks scripts for the NervusDB project.

## Installation

Run the following command from the project root to install the hooks:

```bash
make install-hooks
```

## Hooks

- **pre-commit**: Runs `cargo fmt`, `cargo clippy`, and quick tests before allowing commit
- **pre-push**: Runs full test suite before allowing push

## Manual Testing

You can manually test the hooks without committing/pushing:

```bash
# Test pre-commit checks
make pre-commit

# Test full test suite
make test
```

## Customization

Edit the scripts in this directory to customize the behavior. After editing, run `make install-hooks` again to update the hooks in `.git/hooks/`.
