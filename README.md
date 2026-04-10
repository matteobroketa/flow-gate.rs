# flow-gate.rs

A pure-Rust implementation of the ISAC [Gating-ML 2.0](https://floreada.io/gatingml) standard for flow cytometry gate evaluation.

Named to align with the [`flow-fcs`](https://crates.io/crates/flow-fcs) ecosystem.

## Quick Start

```toml
[dependencies]
flow-gate-core = { git = "https://github.com/matteothebroketa/flow-gate.rs" }
flow-gate-xml  = { git = "https://github.com/matteothebroketa/flow-gate.rs" }
```

```rust
use flow_gate_core::{EventMatrix, FlowGateDocument, ParameterName};

let xml = std::fs::read_to_string("gates.xml")?;
let doc = FlowGateDocument::parse_str(&xml)?;
let matrix = EventMatrix::from_columns(data, param_names)?;
let results = doc.classify_view(&matrix)?;
```

## Validation

The official ISAC Gating-ML 2.0 compliance test corpus is bundled in this repository. Run:

```bash
cargo build --release -p flow-gate --bin flow_gate_compliance_runner
target/release/flow_gate_compliance_runner \
    --root validation-suite/data \
    --output-json report.json
```

The authoritative specification archive is preserved at `official-spec/GatingML_2.0_Specification.20130122.full.zip`.

See [PROJECT_OVERVIEW.md](PROJECT_OVERVIEW.md) for the complete architecture, feature matrix, and compliance status.

## Crates

| Crate | Purpose |
|---|---|
| [`flow-gate-core`](crates/flow-gate-core) | Pure math core — transforms, gates, event matrices |
| [`flow-gate-xml`](crates/flow-gate-xml) | Gating-ML 2.0 XML parsing & serialization |
| [`flow-gate`](crates/flow-gate) | Facade + compliance runner + benchmark binaries |
| [`flow-gate-ffi`](crates/flow-gate-ffi) | C-ABI layer for Python/R bindings |

## License

MIT
