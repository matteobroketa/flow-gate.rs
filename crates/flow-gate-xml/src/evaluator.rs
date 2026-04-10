use std::collections::{HashMap, HashSet};

use flow_gate_core::{EventMatrix, EventMatrixView, FlowGateError, Gate, ParameterName};

use crate::{parse_bound_dimension, BoundDimension, FlowGateDocument, SpectrumMatrixSpec};

pub(crate) fn prepare_owned_matrix(
    doc: &FlowGateDocument,
    matrix: &EventMatrix,
    fcs_compensation: Option<&SpectrumMatrixSpec>,
) -> Result<EventMatrix, FlowGateError> {
    let raw_columns = raw_columns_from_owned(matrix);
    prepare_from_raw_columns(doc, raw_columns, matrix.n_events, fcs_compensation)
}

pub(crate) fn prepare_matrix_from_view(
    doc: &FlowGateDocument,
    matrix: &EventMatrixView<'_>,
    fcs_compensation: Option<&SpectrumMatrixSpec>,
) -> Result<EventMatrix, FlowGateError> {
    let mut raw_columns: HashMap<String, Vec<f64>> = HashMap::new();
    for (idx, name) in matrix.param_names().iter().enumerate() {
        let mut col = Vec::with_capacity(matrix.n_events);
        for event_idx in 0..matrix.n_events {
            if let Some(val) = matrix.value_at(event_idx, idx) {
                col.push(val);
            }
        }
        raw_columns.insert(name.as_str().to_string(), col);
    }
    prepare_from_raw_columns(doc, raw_columns, matrix.n_events, fcs_compensation)
}

fn raw_columns_from_owned(matrix: &EventMatrix) -> HashMap<String, Vec<f64>> {
    let mut out = HashMap::with_capacity(matrix.n_params);
    for (idx, name) in matrix.param_names().iter().enumerate() {
        if let Some(col) = matrix.column(idx) {
            out.insert(name.as_str().to_string(), col.to_vec());
        }
    }
    out
}

fn prepare_from_raw_columns(
    doc: &FlowGateDocument,
    raw_columns: HashMap<String, Vec<f64>>,
    n_events: usize,
    fcs_compensation: Option<&SpectrumMatrixSpec>,
) -> Result<EventMatrix, FlowGateError> {
    let needed_dimensions = collect_needed_dimensions(doc);
    if needed_dimensions.is_empty() {
        return EventMatrix::from_columns(Vec::new(), Vec::new());
    }

    let mut comp_cache: HashMap<String, HashMap<String, Vec<f64>>> = HashMap::new();
    let mut out_names = Vec::with_capacity(needed_dimensions.len());
    let mut out_columns = Vec::with_capacity(needed_dimensions.len());

    for dim in needed_dimensions {
        let col = resolve_dimension_column(
            doc,
            &raw_columns,
            &mut comp_cache,
            n_events,
            fcs_compensation,
            &dim,
        )?;
        out_names.push(dim);
        out_columns.push(col);
    }

    EventMatrix::from_columns(out_columns, out_names)
}

fn collect_needed_dimensions(doc: &FlowGateDocument) -> Vec<ParameterName> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::<ParameterName>::new();
    for gate_id in doc.gate_registry.topological_order() {
        let Some(gate) = doc.gate_registry.get(gate_id) else {
            continue;
        };
        for dim in gate.dimensions() {
            if seen.insert(dim.as_str().to_string()) {
                out.push(dim.clone());
            }
        }
    }
    out
}

fn resolve_dimension_column(
    doc: &FlowGateDocument,
    raw_columns: &HashMap<String, Vec<f64>>,
    comp_cache: &mut HashMap<String, HashMap<String, Vec<f64>>>,
    n_events: usize,
    fcs_compensation: Option<&SpectrumMatrixSpec>,
    dim: &ParameterName,
) -> Result<Vec<f64>, FlowGateError> {
    match parse_bound_dimension(dim) {
        Some(BoundDimension::Fcs {
            compensation_ref,
            name,
        }) => resolve_fcs_column(
            doc,
            raw_columns,
            comp_cache,
            n_events,
            fcs_compensation,
            &compensation_ref,
            &name,
        ),
        Some(BoundDimension::Ratio {
            compensation_ref,
            ratio_id,
        }) => {
            let ratio = doc.ratio_transforms.get(&ratio_id).ok_or_else(|| {
                FlowGateError::InvalidGate(format!("Unknown ratio transformation '{ratio_id}'"))
            })?;

            let x = resolve_fcs_column(
                doc,
                raw_columns,
                comp_cache,
                n_events,
                fcs_compensation,
                &compensation_ref,
                ratio.numerator.as_str(),
            )?;
            let y = resolve_fcs_column(
                doc,
                raw_columns,
                comp_cache,
                n_events,
                fcs_compensation,
                &compensation_ref,
                ratio.denominator.as_str(),
            )?;

            let mut out = Vec::with_capacity(n_events);
            for i in 0..n_events {
                let denom = y[i] - ratio.c;
                if denom == 0.0 || !denom.is_finite() {
                    out.push(f64::NAN);
                } else {
                    // Gating-ML fratio: A * ((x - B) / (y - C)).
                    out.push(ratio.a * ((x[i] - ratio.b) / denom));
                }
            }
            Ok(out)
        }
        None => raw_columns
            .get(dim.as_str())
            .cloned()
            .ok_or_else(|| FlowGateError::UnknownParameter(dim.clone())),
    }
}

fn resolve_fcs_column(
    doc: &FlowGateDocument,
    raw_columns: &HashMap<String, Vec<f64>>,
    comp_cache: &mut HashMap<String, HashMap<String, Vec<f64>>>,
    n_events: usize,
    fcs_compensation: Option<&SpectrumMatrixSpec>,
    compensation_ref: &str,
    name: &str,
) -> Result<Vec<f64>, FlowGateError> {
    if !comp_cache.contains_key(compensation_ref) {
        let prepared = if compensation_ref.eq_ignore_ascii_case("uncompensated") {
            raw_columns.clone()
        } else if compensation_ref.eq_ignore_ascii_case("FCS") {
            if let Some(spec) = fcs_compensation {
                materialize_compensation(raw_columns, n_events, spec)?
            } else {
                // Per Gating-ML 2.0 spec: "FCS (None)" — if the gate references
                // FCS compensation but the FCS file contains no $SPILLOVER keyword,
                // fall back to uncompensated (no compensation applied).
                raw_columns.clone()
            }
        } else {
            let spec = doc.spectrum_matrices.get(compensation_ref).ok_or_else(|| {
                FlowGateError::InvalidGate(format!(
                    "Unknown compensation reference '{compensation_ref}'"
                ))
            })?;
            materialize_compensation(raw_columns, n_events, spec)?
        };
        comp_cache.insert(compensation_ref.to_string(), prepared);
    }

    comp_cache
        .get(compensation_ref)
        .and_then(|m| m.get(name))
        .cloned()
        .ok_or_else(|| FlowGateError::UnknownParameter(ParameterName::from(name.to_string())))
}

fn materialize_compensation(
    raw_columns: &HashMap<String, Vec<f64>>,
    n_events: usize,
    spec: &SpectrumMatrixSpec,
) -> Result<HashMap<String, Vec<f64>>, FlowGateError> {
    let n = spec.n_rows();
    let m = spec.n_cols();
    if n == 0 || m == 0 {
        return Err(FlowGateError::InvalidGate(format!(
            "Compensation matrix '{}' must be non-empty",
            spec.id
        )));
    }
    if spec.coefficients.len() != n * m {
        return Err(FlowGateError::InvalidGate(format!(
            "Compensation matrix '{}' has {} coefficients, expected {}",
            spec.id,
            spec.coefficients.len(),
            n * m
        )));
    }

    let mut detector_cols = Vec::with_capacity(m);
    for det in &spec.detectors {
        let Some(col) = raw_columns.get(det.as_str()) else {
            return Err(FlowGateError::UnknownParameter(det.clone()));
        };
        detector_cols.push(col);
    }

    let unmixing = if spec.matrix_inverted_already {
        spec.coefficients.clone()
    } else if n <= m {
        // Least-squares unmixing matrix U = (S S^T)^(-1) S for S in R^{n x m}.
        // Applied as row-event dot products y_i = sum_j U[i,j] * x_j, this matches
        // flowCore/FlowKit compensation semantics x_row * solve(S) when S is square
        // because U collapses to S^{-T}.
        let s_t = transpose(&spec.coefficients, n, m);
        let ss_t = mat_mul(&spec.coefficients, n, m, &s_t, n);
        let inv = invert_square(&ss_t, n).map_err(|e| {
            FlowGateError::InvalidGate(format!(
                "Cannot invert S*S^T for compensation matrix '{}': {e}",
                spec.id
            ))
        })?;
        mat_mul(&inv, n, n, &spec.coefficients, m)
    } else {
        return Err(FlowGateError::InvalidGate(format!(
            "Compensation matrix '{}' has invalid shape {}x{}; expected n<=m",
            spec.id, n, m
        )));
    };

    let mut out = raw_columns.clone();
    for (row_idx, fluor) in spec.fluorochromes.iter().enumerate() {
        let mut values = vec![0.0_f64; n_events];
        for (event_idx, value) in values.iter_mut().enumerate().take(n_events) {
            let mut acc = 0.0_f64;
            for col_idx in 0..m {
                acc += unmixing[row_idx * m + col_idx] * detector_cols[col_idx][event_idx];
            }
            *value = acc;
        }
        out.insert(fluor.as_str().to_string(), values);
    }
    Ok(out)
}

fn transpose(values: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    let mut out = vec![0.0_f64; rows * cols];
    for r in 0..rows {
        for c in 0..cols {
            out[c * rows + r] = values[r * cols + c];
        }
    }
    out
}

fn mat_mul(a: &[f64], a_rows: usize, a_cols: usize, b: &[f64], b_cols: usize) -> Vec<f64> {
    let mut out = vec![0.0_f64; a_rows * b_cols];
    for r in 0..a_rows {
        for c in 0..b_cols {
            let mut acc = 0.0_f64;
            for k in 0..a_cols {
                acc += a[r * a_cols + k] * b[k * b_cols + c];
            }
            out[r * b_cols + c] = acc;
        }
    }
    out
}

fn invert_square(values: &[f64], n: usize) -> Result<Vec<f64>, String> {
    if values.len() != n * n {
        return Err(format!(
            "matrix has {} entries, expected {}",
            values.len(),
            n * n
        ));
    }
    let mut a = values.to_vec();
    let mut inv = vec![0.0_f64; n * n];
    for i in 0..n {
        inv[i * n + i] = 1.0;
    }

    for col in 0..n {
        let mut pivot_row = col;
        let mut best = a[col * n + col].abs();
        for r in (col + 1)..n {
            let cand = a[r * n + col].abs();
            if cand > best {
                best = cand;
                pivot_row = r;
            }
        }
        if best <= 1e-14 || !best.is_finite() {
            return Err("matrix is singular".to_string());
        }

        if pivot_row != col {
            for c in 0..n {
                a.swap(col * n + c, pivot_row * n + c);
                inv.swap(col * n + c, pivot_row * n + c);
            }
        }

        let pivot = a[col * n + col];
        for c in 0..n {
            a[col * n + c] /= pivot;
            inv[col * n + c] /= pivot;
        }

        for r in 0..n {
            if r == col {
                continue;
            }
            let factor = a[r * n + col];
            if factor == 0.0 {
                continue;
            }
            for c in 0..n {
                a[r * n + c] -= factor * a[col * n + c];
                inv[r * n + c] -= factor * inv[col * n + c];
            }
        }
    }

    Ok(inv)
}
