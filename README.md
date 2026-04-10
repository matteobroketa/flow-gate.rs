# flow-gate.rs

A pure-Rust implementation of the ISAC [Gating-ML 2.0](https://floreada.io/gatingml) standard for flow cytometry gate evaluation.

Named to align with the [`flow-fcs`](https://crates.io/crates/flow-fcs) ecosystem.

---

### OVERVIEW

`flow-gate.rs` provides a comprehensive, high-performance API for parsing Gating-ML 2.0 XML documents and evaluating gates against flow cytometry event data. Built on top of Rayon for parallel processing and BitVec for memory-efficient set operations, the library offers SIMD-ready numerical transforms, zero-copy matrix views for FFI integration, and full support for all Gating-ML 2.0 gate types.

### FEATURES

* **Full Gating-ML 2.0 Standard Support:**
  * Rectangle, Polygon, Ellipsoid, and Boolean gates
  * Parent/child gate hierarchies with topological ordering
  * All standard transforms: Logicle, FASinh, Logarithmic, Linear, Hyperlog
* **High Performance:**
  * Parallel gate evaluation via Rayon
  * Memory-efficient bit-vector set operations
  * Zero-copy matrix views for FFI integration
  * Col-major and row-major layout support
* **Gating-ML 2.0 XML Parsing & Serialization:**
  * Full roundtrip: parse → modify → serialize
  * Namespace-aware parsing with URI-based prefix resolution
  * Compensation (spectrum matrix) and ratio transformation support
* **Type Safety:** Strong typing throughout with clear, descriptive error messages
* **FFI Ready:** C-ABI layer for Python/R bindings (`flow-gate-ffi`)
* **Compliance Testing:** Official ISAC Gating-ML 2.0 test corpus validation

### INSTALLATION

Add this to your `Cargo.toml`:

```toml
[dependencies]
flow-gate-core = "0.1.0"
flow-gate-xml  = "0.1.0"
```

Or use the facade crate (includes compliance runner binaries):

```toml
[dependencies]
flow-gate = "0.1.0"
```

### OPTIONAL FEATURES

* `simd`: Enable SIMD-accelerated numerical transforms
* `nalgebra`: Use `nalgebra` for linear algebra operations in ellipsoid gates (instead of the default pure-Rust implementation)

```toml
[dependencies]
flow-gate-core = { version = "0.1.0", features = ["nalgebra"] }
```

### QUICK START

**PARSING A GATING-ML DOCUMENT**

```rust
use flow_gate_xml::FlowGateDocument;

let xml = std::fs::read_to_string("gates.xml")?;
let doc = FlowGateDocument::parse_str(&xml)?;

// Iterate over all gates
for (gate_id, gate) in doc.gates() {
    println!("Gate: {} — {} dimensions", gate_id, gate.dimensions().len());
}
```

**EVALUATING GATES AGAINST EVENT DATA**

```rust
use flow_gate_core::{EventMatrix, ParameterName};
use flow_gate_xml::FlowGateDocument;

let xml = std::fs::read_to_string("gates.xml")?;
let doc = FlowGateDocument::parse_str(&xml)?;

// Build an event matrix from column vectors
let matrix = EventMatrix::from_columns(
    vec![
        vec![1.0, 2.0, 3.0, 4.0],  // FSC-A
        vec![5.0, 6.0, 7.0, 8.0],  // SSC-A
    ],
    vec![ParameterName::from("FSC-A"), ParameterName::from("SSC-A")],
)?;

// Classify all events against all gates
let results = doc.classify(&matrix)?;

for (gate_id, membership) in &results {
    let inside = membership.count_ones();
    println!("Gate {}: {} events inside", gate_id, inside);
}
```

**SERIALIZING BACK TO XML**

```rust
use flow_gate_xml::FlowGateDocument;

let xml = std::fs::read_to_string("gates.xml")?;
let doc = FlowGateDocument::parse_str(&xml)?;

// Roundtrip: parse → serialize
let output = doc.to_xml()?;
assert!(output.contains("Gating-ML"));
```

**USING THE FFI LAYER**

```rust
use flow_gate_ffi::{MatrixView, MatrixLayout};

// SAFETY: `data` must remain valid for the lifetime of the view.
let data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0];
let view = unsafe {
    MatrixView::from_raw(data.as_ptr(), 2, 2, MatrixLayout::ColMajor)
};
// Use the view from FFI-bound code...
```

### API OVERVIEW

**CORE TYPES**

| Type | Crate | Purpose |
|---|---|---|
| `EventMatrix` | `flow-gate-core` | Owned columnar event data with transform dispatch |
| `EventMatrixView` | `flow-gate-core` | Non-owning read-only matrix view (FFI-friendly) |
| `FlowGateDocument` | `flow-gate-xml` | Parsed Gating-ML 2.0 document with gate registry |
| `GateKind` | `flow-gate-core` | Enum: Rectangle, Polygon, Ellipsoid, Boolean gates |
| `TransformKind` | `flow-gate-core` | Enum: Logicle, FASinh, Log, Lin, Hyperlog |
| `FlowGateError` | `flow-gate-core` | Comprehensive error type for all operations |

**KEY METHODS**

*Document Operations*
* `FlowGateDocument::parse_str(xml)`: Parse Gating-ML 2.0 XML
* `doc.to_xml()`: Serialize back to Gating-ML 2.0 XML
* `doc.gates()`: Iterate over all gates with their IDs
* `doc.classify(matrix)`: Evaluate all gates against event data

*Data Access*
* `EventMatrix::from_columns(columns, names)`: Build from column vectors
* `EventMatrix::from_view(view, names)`: Create from FFI matrix view
* `matrix.column(idx)`: Get a parameter column slice
* `matrix.apply_transform_inplace(transforms)`: Apply transforms to columns

*Gate Evaluation*
* `registry.classify_all(matrix)`: Classify all events against all gates (returns `HashMap<GateId, BitVec>`)
* `gate.classify(matrix, registry)`: Classify against a single gate
* Gates are evaluated in topological order, respecting parent/child hierarchies

### PERFORMANCE

The library is optimized for throughput:

* **Parallel evaluation:** Gate evaluation uses Rayon for multi-threaded execution
* **Bit-vector sets:** Gate membership is stored in `BitVec<u64>`, enabling efficient set operations
* **Zero-copy views:** `MatrixView` enables FFI callers to pass data without copying
* **Columnar storage:** Event data is stored column-major, matching FCS file layout for cache-friendly access
* **SIMD-ready:** Numerical transforms can leverage SIMD via the `simd` feature

### GATING-ML 2.0 STANDARD SUPPORT

The library implements the full Gating-ML 2.0 specification:

| Feature | Status |
|---|---|
| Rectangle gates | ✅ |
| Polygon gates (winding-number) | ✅ |
| Ellipsoid gates (generalized inverse) | ✅ |
| Boolean gates (and/or/not) | ✅ |
| Parent/child hierarchies | ✅ |
| Logicle transform | ✅ |
| FASinh transform | ✅ |
| Logarithmic transform | ✅ |
| Linear transform | ✅ |
| Hyperlog transform | ✅ |
| Compensation (spectrum matrices) | ✅ |
| Ratio transformations | ✅ |
| XML roundtrip | ✅ |

### ERROR HANDLING

The library uses `flow_gate_core::FlowGateError` (backed by `thiserror`) for all operations, providing detailed, actionable error messages for:

* Invalid transform parameters
* Malformed gate definitions
* Unknown gate/parameter references
* XML parse errors with line/column info
* Dimension mismatches
* Cyclic gate hierarchies
* Non-positive-definite covariance matrices

### CRATES

| Crate | Purpose |
|---|---|
| [`flow-gate-core`](crates/flow-gate-core) | Pure math core — transforms, gates, event matrices |
| [`flow-gate-xml`](crates/flow-gate-xml) | Gating-ML 2.0 XML parsing & serialization |
| [`flow-gate`](crates/flow-gate) | Facade + compliance runner + benchmark binaries |
| [`flow-gate-ffi`](crates/flow-gate-ffi) | C-ABI layer for Python/R bindings |

### VALIDATION

The official ISAC Gating-ML 2.0 compliance test corpus is available from the
[ISAC Gating-ML specification download](https://fcsfiles.isac-net.org/).

To validate, download the spec archive, extract the `Compliance tests` directory
to `validation-suite/data/`, then run:

```bash
cargo build --release -p flow-gate --bin flow_gate_compliance_runner
target/release/flow_gate_compliance_runner \
    --root validation-suite/data \
    --output-json report.json
```

### CONTRIBUTING

Contributions are welcome! Please feel free to submit a Pull Request.

### LICENSE

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

### ACKNOWLEDGMENTS

* Built with Rayon for parallel gate evaluation
* Uses `bitvec` for memory-efficient set operations
* Uses `quick-xml` for namespace-aware XML parsing
* Inspired by the ISAC Gating-ML 2.0 specification and the need for fast, pure-Rust gate evaluation

### RELATED PROJECTS

* [`flow-fcs`](https://crates.io/crates/flow-fcs): High-performance FCS file parser built on Polars
* Polars: Fast DataFrame library
* Gating-ML 2.0 Specification: ISAC standard for flow cytometry gate definitions
