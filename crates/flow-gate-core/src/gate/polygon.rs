use crate::error::FlowGateError;
use crate::traits::{Gate, GateId, ParameterName};
use crate::transform::TransformKind;

#[derive(Debug, Clone)]
pub struct PolygonDimension {
    pub parameter: ParameterName,
    pub transform: Option<TransformKind>,
}

#[derive(Debug, Clone)]
pub struct PolygonGate {
    id: GateId,
    parent_id: Option<GateId>,
    x_dim: PolygonDimension,
    y_dim: PolygonDimension,
    dim_names: [ParameterName; 2],
    vertices: Vec<(f64, f64)>,
    aabb_x: (f64, f64),
    aabb_y: (f64, f64),
}

impl PolygonGate {
    pub fn new(
        id: GateId,
        parent_id: Option<GateId>,
        x_dim: PolygonDimension,
        y_dim: PolygonDimension,
        vertices: Vec<(f64, f64)>,
    ) -> Result<Self, FlowGateError> {
        if vertices.len() < 3 {
            return Err(FlowGateError::InvalidGate(
                "PolygonGate requires at least 3 vertices".to_string(),
            ));
        }
        let aabb_x = (
            vertices
                .iter()
                .map(|(x, _)| *x)
                .fold(f64::INFINITY, f64::min),
            vertices
                .iter()
                .map(|(x, _)| *x)
                .fold(f64::NEG_INFINITY, f64::max),
        );
        let aabb_y = (
            vertices
                .iter()
                .map(|(_, y)| *y)
                .fold(f64::INFINITY, f64::min),
            vertices
                .iter()
                .map(|(_, y)| *y)
                .fold(f64::NEG_INFINITY, f64::max),
        );
        let dim_names = [x_dim.parameter.clone(), y_dim.parameter.clone()];
        Ok(Self {
            id,
            parent_id,
            x_dim,
            y_dim,
            dim_names,
            vertices,
            aabb_x,
            aabb_y,
        })
    }

    pub fn x_dim(&self) -> &PolygonDimension {
        &self.x_dim
    }

    pub fn y_dim(&self) -> &PolygonDimension {
        &self.y_dim
    }

    pub fn vertices(&self) -> &[(f64, f64)] {
        &self.vertices
    }
}

impl Gate for PolygonGate {
    fn dimensions(&self) -> &[ParameterName] {
        &self.dim_names
    }

    fn contains(&self, coords: &[f64]) -> bool {
        if coords.len() != 2 {
            return false;
        }
        let (px, py) = (coords[0], coords[1]);
        if !px.is_finite() || !py.is_finite() {
            return false;
        }
        if px < self.aabb_x.0 || px > self.aabb_x.1 || py < self.aabb_y.0 || py > self.aabb_y.1 {
            return false;
        }

        for i in 0..self.vertices.len() {
            let (x0, y0) = self.vertices[i];
            let (x1, y1) = self.vertices[(i + 1) % self.vertices.len()];
            if point_on_segment(px, py, x0, y0, x1, y1) {
                return true;
            }
        }

        // Use winding-number crossing parity, matching flowCore/flowutils
        // behavior for non-simple polygons in the official corpus.
        winding_number(px, py, &self.vertices).abs() % 2 == 1
    }

    fn gate_id(&self) -> &GateId {
        &self.id
    }

    fn parent_id(&self) -> Option<&GateId> {
        self.parent_id.as_ref()
    }
}

#[inline]
pub fn is_left(x0: f64, y0: f64, x1: f64, y1: f64, px: f64, py: f64) -> f64 {
    (x1 - x0) * (py - y0) - (px - x0) * (y1 - y0)
}

/// Epsilon tolerance for polygon edge classification.
/// Polygon gates in the Gating-ML test corpus use transformed coordinates
/// (Logicle, ASinH, etc.) where floating-point precision loss on the
/// coordinate transform causes boundary points to drift by ~1e-10–1e-8.
/// Using a generous tolerance avoids misclassifying near-boundary events.
const POLYGON_EDGE_EPS: f64 = 1e-9;

pub fn winding_number(px: f64, py: f64, vertices: &[(f64, f64)]) -> i32 {
    let n = vertices.len();
    let mut wn = 0_i32;
    for i in 0..n {
        let (x0, y0) = vertices[i];
        let (x1, y1) = vertices[(i + 1) % n];

        // Points extremely close to an edge are classified as inside
        // to avoid floating-point flip-flop on transformed coordinates.
        let cp = is_left(x0, y0, x1, y1, px, py);
        if cp.abs() < POLYGON_EDGE_EPS {
            return 1;
        }

        if y0 <= py {
            if y1 > py && cp > 0.0 {
                wn += 1;
            }
        } else if y1 <= py && cp < 0.0 {
            wn -= 1;
        }
    }
    wn
}

/// Checks whether a point lies on a line segment, with tolerance.
fn point_on_segment(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let cross = is_left(x0, y0, x1, y1, px, py).abs();
    if cross > POLYGON_EDGE_EPS {
        return false;
    }
    let min_x = x0.min(x1) - POLYGON_EDGE_EPS;
    let max_x = x0.max(x1) + POLYGON_EDGE_EPS;
    let min_y = y0.min(y1) - POLYGON_EDGE_EPS;
    let max_y = y0.max(y1) + POLYGON_EDGE_EPS;
    px >= min_x && px <= max_x && py >= min_y && py <= max_y
}
