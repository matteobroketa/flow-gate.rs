# Contributing to flow-gate.rs

Thank you for contributing.

## Development setup

- Rust `>= 1.75` (MSRV)
- Recommended: current stable Rust

## Local quality checks

Run these before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Or use aliases from `.cargo/config.toml`:

```bash
cargo xcheck
cargo xlint
cargo xtest
cargo xdoc
```

## Scope guardrails

- Do not change calculation semantics unless the PR explicitly targets a validated numerical bug.
- Any change touching transforms, compensation, or gate classification must include tests.
- Keep behavior-compatible refactors separate from logic changes.

## Compliance runner

The compliance runner expects the ISAC corpus to be present under a root directory and is run with:

```bash
cargo run -p flow-gate --bin flow_gate_compliance_runner -- --root <path> --output-json report.json
```

## Pull requests

- Keep PRs focused and small.
- Include a concise change summary and testing notes.
- If behavior changes are intentional, document rationale and expected impact.
