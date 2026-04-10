use std::collections::HashMap;
use std::marker::PhantomData;

use rayon::prelude::*;
use smallvec::SmallVec;

use crate::error::FlowGateError;
use crate::traits::{ParameterName, Transform};
use crate::transform::TransformKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixLayout {
    RowMajor,
    ColMajor,
}

/// Non-owning read-only matrix view used by FFI bridges.
#[derive(Debug, Clone, Copy)]
pub struct MatrixView<'a> {
    pub ptr: *const f64,
    pub n_rows: usize,
    pub n_cols: usize,
    pub layout: MatrixLayout,
    _lifetime: PhantomData<&'a f64>,
}

// SAFETY: MatrixView is read-only and caller upholds pointer/lifetime validity.
unsafe impl<'a> Send for MatrixView<'a> {}
// SAFETY: MatrixView is read-only and caller upholds pointer/lifetime validity.
unsafe impl<'a> Sync for MatrixView<'a> {}

impl<'a> MatrixView<'a> {
    /// SAFETY: Caller must guarantee the pointer points to at least `n_rows * n_cols` valid f64 values.
    pub unsafe fn from_raw(
        ptr: *const f64,
        n_rows: usize,
        n_cols: usize,
        layout: MatrixLayout,
    ) -> Self {
        Self {
            ptr,
            n_rows,
            n_cols,
            layout,
            _lifetime: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, row: usize, col: usize) -> f64 {
        match self.layout {
            MatrixLayout::RowMajor => *self.ptr.add(row * self.n_cols + col),
            MatrixLayout::ColMajor => *self.ptr.add(col * self.n_rows + row),
        }
    }

    pub fn column(&self, col: usize) -> ColumnIter<'a, '_> {
        ColumnIter {
            view: self,
            col,
            row: 0,
        }
    }
}

pub struct ColumnIter<'a, 'v> {
    view: &'v MatrixView<'a>,
    col: usize,
    row: usize,
}

impl<'a, 'v> Iterator for ColumnIter<'a, 'v> {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row >= self.view.n_rows {
            return None;
        }
        let row = self.row;
        self.row += 1;
        // SAFETY: bounds checked above.
        Some(unsafe { self.view.get_unchecked(row, self.col) })
    }
}

pub struct EventMatrixView<'a> {
    view: MatrixView<'a>,
    pub n_events: usize,
    pub n_params: usize,
    param_names: Vec<ParameterName>,
    param_index: HashMap<ParameterName, usize>,
}

impl<'a> EventMatrixView<'a> {
    pub fn project_indices(
        &self,
        names: &[ParameterName],
    ) -> Result<SmallVec<[usize; 8]>, FlowGateError> {
        let mut indices = SmallVec::<[usize; 8]>::with_capacity(names.len());
        for name in names {
            let Some(&idx) = self.param_index.get(name) else {
                return Err(FlowGateError::UnknownParameter(name.clone()));
            };
            indices.push(idx);
        }
        Ok(indices)
    }

    #[inline]
    pub fn value_at(&self, event_idx: usize, param_idx: usize) -> f64 {
        debug_assert!(event_idx < self.n_events);
        debug_assert!(param_idx < self.n_params);
        // SAFETY: indices are validated by caller and debug assertions above.
        unsafe { self.view.get_unchecked(event_idx, param_idx) }
    }

    pub fn param_names(&self) -> &[ParameterName] {
        &self.param_names
    }
}

pub struct EventMatrix {
    pub n_events: usize,
    pub n_params: usize,
    data: Vec<f64>,
    param_names: Vec<ParameterName>,
    param_index: HashMap<ParameterName, usize>,
}

impl EventMatrix {
    pub fn new(
        n_events: usize,
        n_params: usize,
        data: Vec<f64>,
        param_names: Vec<ParameterName>,
    ) -> Result<Self, FlowGateError> {
        if data.len() != n_events.saturating_mul(n_params) {
            return Err(FlowGateError::InvalidGate(format!(
                "EventMatrix data length {} does not match n_events*n_params {}",
                data.len(),
                n_events.saturating_mul(n_params)
            )));
        }
        if param_names.len() != n_params {
            return Err(FlowGateError::InvalidGate(format!(
                "EventMatrix param_names length {} does not match n_params {}",
                param_names.len(),
                n_params
            )));
        }
        let mut param_index = HashMap::with_capacity(param_names.len());
        for (i, name) in param_names.iter().enumerate() {
            param_index.insert(name.clone(), i);
        }
        Ok(Self {
            n_events,
            n_params,
            data,
            param_names,
            param_index,
        })
    }

    pub fn from_columns(
        columns: Vec<Vec<f64>>,
        param_names: Vec<ParameterName>,
    ) -> Result<Self, FlowGateError> {
        let n_params = columns.len();
        let n_events = columns.first().map_or(0, Vec::len);
        if columns.iter().any(|c| c.len() != n_events) {
            return Err(FlowGateError::InvalidGate(
                "All EventMatrix columns must have identical length".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(n_events.saturating_mul(n_params));
        for col in columns {
            data.extend_from_slice(&col);
        }
        Self::new(n_events, n_params, data, param_names)
    }

    pub fn from_view<'a>(
        view: MatrixView<'a>,
        param_names: Vec<ParameterName>,
    ) -> Result<EventMatrixView<'a>, FlowGateError> {
        if param_names.len() != view.n_cols {
            return Err(FlowGateError::DimensionMismatch(
                param_names.len(),
                view.n_cols,
            ));
        }
        let mut param_index = HashMap::with_capacity(param_names.len());
        for (i, name) in param_names.iter().enumerate() {
            param_index.insert(name.clone(), i);
        }
        Ok(EventMatrixView {
            view,
            n_events: view.n_rows,
            n_params: view.n_cols,
            param_names,
            param_index,
        })
    }

    pub fn data(&self) -> &[f64] {
        &self.data
    }

    pub fn param_names(&self) -> &[ParameterName] {
        &self.param_names
    }

    pub fn column(&self, column_index: usize) -> Option<&[f64]> {
        if column_index >= self.n_params {
            return None;
        }
        let start = column_index * self.n_events;
        let end = start + self.n_events;
        Some(&self.data[start..end])
    }

    pub fn project(&self, names: &[ParameterName]) -> Result<ProjectedMatrix<'_>, FlowGateError> {
        let mut columns = SmallVec::<[&[f64]; 4]>::with_capacity(names.len());
        for name in names {
            let Some(&idx) = self.param_index.get(name) else {
                return Err(FlowGateError::UnknownParameter(name.clone()));
            };
            let start = idx * self.n_events;
            let end = start + self.n_events;
            columns.push(&self.data[start..end]);
        }
        Ok(ProjectedMatrix {
            n_events: self.n_events,
            n_cols: names.len(),
            columns,
        })
    }

    /// Deviation approved by user: transform dispatch uses `TransformKind` instead of `&dyn Transform`
    /// because `Transform: Clone` is not object-safe.
    pub fn apply_transforms_inplace(&mut self, transforms: &[(usize, TransformKind)]) {
        let transform_map: HashMap<usize, TransformKind> = transforms.iter().copied().collect();
        self.data
            .par_chunks_mut(self.n_events.max(1))
            .enumerate()
            .for_each(|(col_idx, col)| {
                if let Some(transform) = transform_map.get(&col_idx) {
                    for value in col {
                        *value = transform.apply(*value);
                    }
                }
            });
    }

    pub fn events(&self) -> impl Iterator<Item = SmallVec<[f64; 8]>> + '_ {
        (0..self.n_events).map(|event_idx| {
            let mut row = SmallVec::<[f64; 8]>::with_capacity(self.n_params);
            for col_idx in 0..self.n_params {
                let offset = col_idx * self.n_events + event_idx;
                row.push(self.data[offset]);
            }
            row
        })
    }
}

pub struct ProjectedMatrix<'a> {
    pub(crate) n_events: usize,
    pub(crate) n_cols: usize,
    pub(crate) columns: SmallVec<[&'a [f64]; 4]>,
}

impl<'a> ProjectedMatrix<'a> {
    pub fn n_events(&self) -> usize {
        self.n_events
    }

    pub fn n_cols(&self) -> usize {
        self.n_cols
    }

    pub fn columns(&self) -> &[&'a [f64]] {
        &self.columns
    }

    pub fn events(&'a self) -> impl Iterator<Item = SmallVec<[f64; 4]>> + 'a {
        (0..self.n_events).map(|event_idx| {
            let mut values = SmallVec::<[f64; 4]>::with_capacity(self.n_cols);
            for col in &self.columns {
                values.push(col[event_idx]);
            }
            values
        })
    }
}
