use smallvec::SmallVec;

use crate::error::FlowGateError;
use crate::traits::{Gate, GateId, ParameterName};
use crate::transform::TransformKind;

#[derive(Debug, Clone)]
pub struct RectangleDimension {
    pub parameter: ParameterName,
    pub transform: Option<TransformKind>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct RectangleGate {
    id: GateId,
    parent_id: Option<GateId>,
    dimensions: SmallVec<[RectangleDimension; 4]>,
    dim_names: SmallVec<[ParameterName; 4]>,
}

impl RectangleGate {
    pub fn new(
        id: GateId,
        parent_id: Option<GateId>,
        dimensions: Vec<RectangleDimension>,
    ) -> Result<Self, FlowGateError> {
        if dimensions.is_empty() {
            return Err(FlowGateError::InvalidGate(
                "RectangleGate requires at least one dimension".to_string(),
            ));
        }
        let dimensions: SmallVec<[RectangleDimension; 4]> = dimensions.into();
        let dim_names = dimensions.iter().map(|d| d.parameter.clone()).collect();
        Ok(Self {
            id,
            parent_id,
            dimensions,
            dim_names,
        })
    }

    pub fn rectangle_dimensions(&self) -> &[RectangleDimension] {
        &self.dimensions
    }
}

impl Gate for RectangleGate {
    fn dimensions(&self) -> &[ParameterName] {
        &self.dim_names
    }

    fn contains(&self, coords: &[f64]) -> bool {
        if coords.len() != self.dimensions.len() {
            return false;
        }
        coords
            .iter()
            .zip(self.dimensions.iter())
            .all(|(&coord, dim)| {
                if !coord.is_finite() {
                    return false;
                }
                let above_min = dim.min.is_none_or(|lo| coord >= lo);
                // Gating-ML ranges are [min, max), i.e. inclusive min and exclusive max.
                let below_max = dim.max.is_none_or(|hi| coord < hi);
                above_min && below_max
            })
    }

    fn gate_id(&self) -> &GateId {
        &self.id
    }

    fn parent_id(&self) -> Option<&GateId> {
        self.parent_id.as_ref()
    }
}
