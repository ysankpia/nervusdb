# Git Hooks for NervusDB

This directory contains git hooks scripts for the NervusDB project.

## Installation

Run the following command from the project root to install the hooks:

```bash
make install-hooks
```

## Hooks

- **pre-commit**: Runs `bash scripts/check.sh`.
- **pre-push**: Runs `bash scripts/check.sh`.

The hooks intentionally do not run full historical tests. Use
`bash scripts/workspace_full_test.sh` manually for release preparation, broad
refactors, or changes that intentionally touch frozen/experimental surfaces.

## Manual Testing

You can manually test the hooks without committing/pushing:

```bash
# Test pre-commit checks
make pre-commit

# Run full historical workspace verification manually
bash scripts/workspace_full_test.sh
```

## Customization

Edit the scripts in this directory to customize the behavior. After editing, run `make install-hooks` again to update the hooks in `.git/hooks/`.
