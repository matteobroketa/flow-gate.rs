use smallvec::SmallVec;

use crate::error::FlowGateError;
use crate::traits::{Gate, GateId, ParameterName};
use crate::transform::TransformKind;

#[derive(Debug, Clone)]
pub struct EllipsoidDimension {
    pub parameter: ParameterName,
    pub transform: Option<TransformKind>,
}

#[derive(Debug, Clone)]
pub struct EllipsoidCovariance {
    metric: EllipsoidMetric,
    matrix_full: Vec<f64>,
    n: usize,
}

#[derive(Debug, Clone)]
enum EllipsoidMetric {
    /// Strict SPD mode using Cholesky factorization.
    Cholesky { l: Vec<f64> },
    /// Compatibility mode matching flowCore/flowutils behavior: quadratic form with inv(S).
    GeneralInverse { inv: Vec<f64> },
}

impl EllipsoidCovariance {
    pub fn from_upper_triangular(upper: &[f64], n: usize) -> Result<Self, FlowGateError> {
        let expected = n * (n + 1) / 2;
        if upper.len() != expected {
            return Err(FlowGateError::DimensionMismatch(upper.len(), expected));
        }

        let mut s = vec![0.0_f64; n * n];
        let mut idx = 0usize;
        for i in 0..n {
            for j in i..n {
                let v = upper[idx];
                idx += 1;
                s[i * n + j] = v;
                s[j * n + i] = v;
            }
        }

        Self::from_symmetric_matrix(&s, n)
    }

    pub fn from_full_matrix(values: &[f64], n: usize) -> Result<Self, FlowGateError> {
        let expected = n * n;
        if values.len() != expected {
            return Err(FlowGateError::DimensionMismatch(values.len(), expected));
        }
        let mut s = values.to_vec();
        for row in 0..n {
            for col in 0..n {
                let a = s[row * n + col];
                let b = s[col * n + row];
                if !a.is_finite() || !b.is_finite() {
                    return Err(FlowGateError::NotPositiveDefinite);
                }
                // Enforce symmetry with tolerance for minor rounding differences.
                if (a - b).abs() > 1e-9 {
                    return Err(FlowGateError::InvalidGate(
                        "Ellipsoid covariance matrix is not symmetric".to_string(),
                    ));
                }
                let avg = 0.5 * (a + b);
                s[row * n + col] = avg;
                s[col * n + row] = avg;
            }
        }
        Self::from_symmetric_matrix(&s, n)
    }

    pub fn from_full_matrix_general(values: &[f64], n: usize) -> Result<Self, FlowGateError> {
        let expected = n * n;
        if values.len() != expected {
            return Err(FlowGateError::DimensionMismatch(values.len(), expected));
        }
        if values.iter().any(|v| !v.is_finite()) {
            return Err(FlowGateError::InvalidGate(
                "Ellipsoid covariance matrix must contain only finite values".to_string(),
            ));
        }

        let inv = invert_square(values, n).map_err(|e| FlowGateError::InvalidGate(e.to_string()))?;
        Ok(Self {
            metric: EllipsoidMetric::GeneralInverse { inv },
            matrix_full: values.to_vec(),
            n,
        })
    }

    fn from_symmetric_matrix(s: &[f64], n: usize) -> Result<Self, FlowGateError> {
        let mut l = vec![0.0_f64; n * n];
        let mut min_diag = f64::INFINITY;
        let mut max_diag = 0.0_f64;
        for i in 0..n {
            for j in 0..=i {
                let mut sum = s[i * n + j];
                for k in 0..j {
                    sum -= l[i * n + k] * l[j * n + k];
                }
                if i == j {
                    if !sum.is_finite() || sum <= 0.0 {
                        return Err(FlowGateError::NotPositiveDefinite);
                    }
                    let diag = sum.sqrt();
                    if !diag.is_finite() || diag <= 0.0 {
                        return Err(FlowGateError::NotPositiveDefinite);
                    }
                    min_diag = min_diag.min(diag);
                    max_diag = max_diag.max(diag);
                    l[i * n + j] = diag;
                } else {
                    let denom = l[j * n + j];
                    if !denom.is_finite() || denom <= 0.0 {
                        return Err(FlowGateError::NotPositiveDefinite);
                    }
                    l[i * n + j] = sum / denom;
                }
            }
        }

        if !min_diag.is_finite() || min_diag <= 0.0 || !max_diag.is_finite() {
            return Err(FlowGateError::NotPositiveDefinite);
        }
        // Cholesky-based condition estimate: cond(S) ~= cond(L)^2.
        // Reject highly ill-conditioned matrices as requested by spec tests.
        let cond_estimate = (max_diag / min_diag).powi(2);
        if !cond_estimate.is_finite() || cond_estimate > 1.0e10 {
            return Err(FlowGateError::NotPositiveDefinite);
        }

        Ok(Self {
            metric: EllipsoidMetric::Cholesky { l },
            matrix_full: s.to_vec(),
            n,
        })
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn uses_general_inverse(&self) -> bool {
        matches!(self.metric, EllipsoidMetric::GeneralInverse { .. })
    }

    pub fn full_matrix(&self) -> &[f64] {
        &self.matrix_full
    }

    pub fn to_upper_triangular(&self) -> Vec<f64> {
        let n = self.n;
        let s = &self.matrix_full;
        let mut upper = Vec::with_capacity(n * (n + 1) / 2);
        for row in 0..n {
            for col in row..n {
                upper.push(s[row * n + col]);
            }
        }
        upper
    }

    pub fn mahalanobis_sq(&self, delta: &[f64]) -> f64 {
        if delta.len() != self.n || delta.iter().any(|v| !v.is_finite()) {
            return f64::NAN;
        }

        match &self.metric {
            EllipsoidMetric::Cholesky { l } => {
                let n = self.n;
                let mut z = SmallVec::<[f64; 8]>::with_capacity(n);
                for i in 0..n {
                    z.push(0.0);
                    let mut rhs = delta[i];
                    for k in 0..i {
                        rhs -= l[i * n + k] * z[k];
                    }
                    let diag = l[i * n + i];
                    if !diag.is_finite() || diag <= 0.0 {
                        return f64::NAN;
                    }
                    z[i] = rhs / diag;
                }
                z.iter().map(|v| v * v).sum()
            }
            EllipsoidMetric::GeneralInverse { inv } => {
                let n = self.n;
                let mut quad = 0.0_f64;
                for row in 0..n {
                    let mut proj = 0.0_f64;
                    for col in 0..n {
                        proj += inv[row * n + col] * delta[col];
                    }
                    quad += delta[row] * proj;
                }
                quad
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EllipsoidGate {
    id: GateId,
    parent_id: Option<GateId>,
    dimensions: Vec<EllipsoidDimension>,
    dim_names: Vec<ParameterName>,
    mean: Vec<f64>,
    covariance: EllipsoidCovariance,
    distance_sq: f64,
}

impl EllipsoidGate {
    pub fn new(
        id: GateId,
        parent_id: Option<GateId>,
        dimensions: Vec<EllipsoidDimension>,
        mean: Vec<f64>,
        covariance_upper: &[f64],
        distance_sq: f64,
    ) -> Result<Self, FlowGateError> {
        let n = dimensions.len();
        if n == 0 {
            return Err(FlowGateError::InvalidGate(
                "EllipsoidGate requires at least one dimension".to_string(),
            ));
        }
        if mean.len() != n {
            return Err(FlowGateError::DimensionMismatch(mean.len(), n));
        }
        if !distance_sq.is_finite() || distance_sq <= 0.0 {
            return Err(FlowGateError::InvalidGate(
                "EllipsoidGate distanceSquare must be finite and > 0".to_string(),
            ));
        }
        let covariance = match covariance_upper.len() {
            len if len == n * (n + 1) / 2 => {
                EllipsoidCovariance::from_upper_triangular(covariance_upper, n)?
            }
            len if len == n * n => EllipsoidCovariance::from_full_matrix(covariance_upper, n)?,
            len => return Err(FlowGateError::DimensionMismatch(len, n * (n + 1) / 2)),
        };
        let dim_names = dimensions.iter().map(|d| d.parameter.clone()).collect();
        Ok(Self {
            id,
            parent_id,
            dimensions,
            dim_names,
            mean,
            covariance,
            distance_sq,
        })
    }

    /// Parser-targeted compatibility constructor for official Gating-ML corpus.
    /// Accepts any finite, invertible full matrix and evaluates via inv(S).
    pub fn new_general_covariance(
        id: GateId,
        parent_id: Option<GateId>,
        dimensions: Vec<EllipsoidDimension>,
        mean: Vec<f64>,
        covariance_full: &[f64],
        distance_sq: f64,
    ) -> Result<Self, FlowGateError> {
        let n = dimensions.len();
        if n == 0 {
            return Err(FlowGateError::InvalidGate(
                "EllipsoidGate requires at least one dimension".to_string(),
            ));
        }
        if mean.len() != n {
            return Err(FlowGateError::DimensionMismatch(mean.len(), n));
        }
        if !distance_sq.is_finite() || distance_sq <= 0.0 {
            return Err(FlowGateError::InvalidGate(
                "EllipsoidGate distanceSquare must be finite and > 0".to_string(),
            ));
        }
        let covariance = EllipsoidCovariance::from_full_matrix_general(covariance_full, n)?;
        let dim_names = dimensions.iter().map(|d| d.parameter.clone()).collect();
        Ok(Self {
            id,
            parent_id,
            dimensions,
            dim_names,
            mean,
            covariance,
            distance_sq,
        })
    }

    pub fn dimensions_def(&self) -> &[EllipsoidDimension] {
        &self.dimensions
    }

    pub fn mean(&self) -> &[f64] {
        &self.mean
    }

    pub fn covariance(&self) -> &EllipsoidCovariance {
        &self.covariance
    }

    pub fn distance_sq(&self) -> f64 {
        self.distance_sq
    }
}

impl Gate for EllipsoidGate {
    fn dimensions(&self) -> &[ParameterName] {
        &self.dim_names
    }

    fn contains(&self, coords: &[f64]) -> bool {
        let n = self.dimensions.len();
        if coords.len() != n {
            return false;
        }
        if coords.iter().any(|c| !c.is_finite() || c.abs() > 1e15) {
            return false;
        }

        let mut delta = SmallVec::<[f64; 8]>::with_capacity(n);
        for (i, &coord) in coords.iter().enumerate() {
            delta.push(coord - self.mean[i]);
        }
        let d_sq = self.covariance.mahalanobis_sq(&delta);
        d_sq.is_finite() && d_sq <= self.distance_sq
    }

    fn gate_id(&self) -> &GateId {
        &self.id
    }

    fn parent_id(&self) -> Option<&GateId> {
        self.parent_id.as_ref()
    }
}

fn invert_square(values: &[f64], n: usize) -> Result<Vec<f64>, &'static str> {
    if values.len() != n * n {
        return Err("matrix size mismatch");
    }

    let mut a = values.to_vec();
    let mut inv = vec![0.0_f64; n * n];
    for i in 0..n {
        inv[i * n + i] = 1.0;
    }

    for col in 0..n {
        let mut pivot_row = col;
        let mut best = a[col * n + col].abs();
        for row in (col + 1)..n {
            let cand = a[row * n + col].abs();
            if cand > best {
                best = cand;
                pivot_row = row;
            }
        }

        if !best.is_finite() || best <= 1.0e-14 {
            return Err("Ellipsoid covariance matrix is singular");
        }

        if pivot_row != col {
            for k in 0..n {
                a.swap(col * n + k, pivot_row * n + k);
                inv.swap(col * n + k, pivot_row * n + k);
            }
        }

        let pivot = a[col * n + col];
        for k in 0..n {
            a[col * n + k] /= pivot;
            inv[col * n + k] /= pivot;
        }

        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = a[row * n + col];
            if factor == 0.0 {
                continue;
            }
            for k in 0..n {
                a[row * n + k] -= factor * a[col * n + k];
                inv[row * n + k] -= factor * inv[col * n + k];
            }
        }
    }

    if inv.iter().any(|v| !v.is_finite()) {
        return Err("Ellipsoid covariance inverse contains non-finite values");
    }
    Ok(inv)
}
