use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use rayon::prelude::*;
use smallvec::SmallVec;

use crate::error::FlowGateError;
use crate::event::{EventMatrix, EventMatrixView};
use crate::gate::{
    BooleanGate, BooleanOp, EllipsoidDimension, EllipsoidGate, PolygonDimension, PolygonGate,
    RectangleDimension, RectangleGate,
};
use crate::traits::{ApplyGate, BitVec, Gate, GateId, ParameterName, Transform};
use crate::transform::TransformKind;

#[derive(Debug, Clone)]
pub enum GateKind {
    Rectangle(RectangleGate),
    Polygon(PolygonGate),
    Ellipsoid(EllipsoidGate),
    Boolean(BooleanGate),
}

impl GateKind {
    fn dependency_ids(&self) -> Vec<GateId> {
        let mut deps = Vec::new();
        if let Some(parent) = self.parent_id() {
            deps.push(parent.clone());
        }
        if let Self::Boolean(g) = self {
            for op in g.operands() {
                deps.push(op.gate_id.clone());
            }
        }
        deps
    }

    fn transforms(&self) -> SmallVec<[Option<TransformKind>; 8]> {
        match self {
            Self::Rectangle(g) => g
                .rectangle_dimensions()
                .iter()
                .map(|d: &RectangleDimension| d.transform)
                .collect(),
            Self::Polygon(g) => {
                let dims: [&PolygonDimension; 2] = [g.x_dim(), g.y_dim()];
                dims.iter().map(|d| d.transform).collect()
            }
            Self::Ellipsoid(g) => g
                .dimensions_def()
                .iter()
                .map(|d: &EllipsoidDimension| d.transform)
                .collect(),
            Self::Boolean(_) => SmallVec::new(),
        }
    }
}

impl Gate for GateKind {
    fn dimensions(&self) -> &[ParameterName] {
        match self {
            Self::Rectangle(g) => g.dimensions(),
            Self::Polygon(g) => g.dimensions(),
            Self::Ellipsoid(g) => g.dimensions(),
            Self::Boolean(g) => g.dimensions(),
        }
    }

    fn contains(&self, coords: &[f64]) -> bool {
        match self {
            Self::Rectangle(g) => g.contains(coords),
            Self::Polygon(g) => g.contains(coords),
            Self::Ellipsoid(g) => g.contains(coords),
            Self::Boolean(g) => g.contains(coords),
        }
    }

    fn gate_id(&self) -> &GateId {
        match self {
            Self::Rectangle(g) => g.gate_id(),
            Self::Polygon(g) => g.gate_id(),
            Self::Ellipsoid(g) => g.gate_id(),
            Self::Boolean(g) => g.gate_id(),
        }
    }

    fn parent_id(&self) -> Option<&GateId> {
        match self {
            Self::Rectangle(g) => g.parent_id(),
            Self::Polygon(g) => g.parent_id(),
            Self::Ellipsoid(g) => g.parent_id(),
            Self::Boolean(g) => g.parent_id(),
        }
    }
}

impl ApplyGate for GateKind {
    fn classify(&self, matrix: &EventMatrix, gate_map: &GateRegistry) -> BitVec {
        match gate_map.classify_all(matrix) {
            Ok(results) => results
                .get(self.gate_id())
                .cloned()
                .unwrap_or_else(|| BitVec::repeat(false, matrix.n_events)),
            Err(_) => BitVec::repeat(false, matrix.n_events),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct GateRegistry {
    gates: IndexMap<GateId, GateKind>,
    topo_order: Vec<GateId>,
}

impl GateRegistry {
    pub fn new(gates: IndexMap<GateId, GateKind>) -> Result<Self, FlowGateError> {
        let topo_order = compute_topological_order(&gates)?;
        Ok(Self { gates, topo_order })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&GateId, &GateKind)> {
        self.gates.iter()
    }

    pub fn get(&self, gate_id: &GateId) -> Option<&GateKind> {
        self.gates.get(gate_id)
    }

    pub fn insert(&mut self, gate_id: GateId, gate: GateKind) -> Result<(), FlowGateError> {
        self.gates.insert(gate_id, gate);
        self.topo_order = compute_topological_order(&self.gates)?;
        Ok(())
    }

    pub fn topological_order(&self) -> &[GateId] {
        &self.topo_order
    }

    pub fn classify_all(
        &self,
        matrix: &EventMatrix,
    ) -> Result<HashMap<GateId, BitVec>, FlowGateError> {
        let n_events = matrix.n_events;
        let mut results: HashMap<GateId, BitVec> = HashMap::with_capacity(self.gates.len());

        for gate_id in &self.topo_order {
            let gate = self.gates.get(gate_id).ok_or_else(|| {
                FlowGateError::UnknownGateReference(gate_id.clone(), gate_id.clone())
            })?;

            let membership = match gate {
                GateKind::Boolean(boolean_gate) => {
                    evaluate_boolean_gate(boolean_gate, &results, n_events)?
                }
                _ => classify_spatial_gate(gate, matrix)?,
            };

            let membership = if let Some(parent_id) = gate.parent_id() {
                let parent_bits = results
                    .get(parent_id)
                    .ok_or_else(|| FlowGateError::MissingParentGate(parent_id.clone()))?;
                let mut child = membership;
                for idx in 0..n_events {
                    let keep = child[idx] & parent_bits[idx];
                    child.set(idx, keep);
                }
                child
            } else {
                membership
            };

            results.insert(gate_id.clone(), membership);
        }

        Ok(results)
    }

    pub fn classify_all_view(
        &self,
        matrix: &EventMatrixView<'_>,
    ) -> Result<HashMap<GateId, BitVec>, FlowGateError> {
        let n_events = matrix.n_events;
        let mut results: HashMap<GateId, BitVec> = HashMap::with_capacity(self.gates.len());

        for gate_id in &self.topo_order {
            let gate = self.gates.get(gate_id).ok_or_else(|| {
                FlowGateError::UnknownGateReference(gate_id.clone(), gate_id.clone())
            })?;

            let membership = match gate {
                GateKind::Boolean(boolean_gate) => {
                    evaluate_boolean_gate(boolean_gate, &results, n_events)?
                }
                _ => classify_spatial_gate_view(gate, matrix)?,
            };

            let membership = if let Some(parent_id) = gate.parent_id() {
                let parent_bits = results
                    .get(parent_id)
                    .ok_or_else(|| FlowGateError::MissingParentGate(parent_id.clone()))?;
                let mut child = membership;
                for idx in 0..n_events {
                    let keep = child[idx] & parent_bits[idx];
                    child.set(idx, keep);
                }
                child
            } else {
                membership
            };

            results.insert(gate_id.clone(), membership);
        }

        Ok(results)
    }
}

fn classify_spatial_gate(gate: &GateKind, matrix: &EventMatrix) -> Result<BitVec, FlowGateError> {
    let n_events = matrix.n_events;
    let projected = matrix.project(gate.dimensions())?;
    let columns = projected.columns();
    let transforms = gate.transforms();

    let bools: Vec<bool> = (0..n_events)
        .into_par_iter()
        .map(|event_idx| {
            let mut coords = SmallVec::<[f64; 8]>::with_capacity(columns.len());
            for (dim_idx, col) in columns.iter().enumerate() {
                let raw = col[event_idx];
                let value = transforms
                    .get(dim_idx)
                    .copied()
                    .flatten()
                    .map_or(raw, |t| t.apply(raw));
                coords.push(value);
            }
            gate.contains(&coords)
        })
        .collect();

    Ok(bools.into_iter().collect())
}

fn classify_spatial_gate_view(
    gate: &GateKind,
    matrix: &EventMatrixView<'_>,
) -> Result<BitVec, FlowGateError> {
    let n_events = matrix.n_events;
    let dim_indices = matrix.project_indices(gate.dimensions())?;
    let transforms = gate.transforms();

    let bools: Vec<bool> = (0..n_events)
        .into_par_iter()
        .map(|event_idx| {
            let mut coords = SmallVec::<[f64; 8]>::with_capacity(dim_indices.len());
            for (dim_idx, &param_idx) in dim_indices.iter().enumerate() {
                let raw = matrix.value_at(event_idx, param_idx);
                let value = transforms
                    .get(dim_idx)
                    .copied()
                    .flatten()
                    .map_or(raw, |t| t.apply(raw));
                coords.push(value);
            }
            gate.contains(&coords)
        })
        .collect();

    Ok(bools.into_iter().collect())
}

fn evaluate_boolean_gate(
    gate: &BooleanGate,
    results: &HashMap<GateId, BitVec>,
    n_events: usize,
) -> Result<BitVec, FlowGateError> {
    let mut operand_bits = Vec::with_capacity(gate.operands().len());
    for operand in gate.operands() {
        let source = results.get(&operand.gate_id).ok_or_else(|| {
            FlowGateError::UnknownGateReference(gate.gate_id().clone(), operand.gate_id.clone())
        })?;
        let mut bits = source.clone();
        if operand.complement {
            for idx in 0..n_events {
                let prev = bits[idx];
                bits.set(idx, !prev);
            }
        }
        operand_bits.push(bits);
    }

    let out = match gate.op() {
        BooleanOp::And => {
            let mut acc = BitVec::repeat(true, n_events);
            for bits in &operand_bits {
                for idx in 0..n_events {
                    let prev = acc[idx];
                    acc.set(idx, prev & bits[idx]);
                }
            }
            acc
        }
        BooleanOp::Or => {
            let mut acc = BitVec::repeat(false, n_events);
            for bits in &operand_bits {
                for idx in 0..n_events {
                    let prev = acc[idx];
                    acc.set(idx, prev | bits[idx]);
                }
            }
            acc
        }
        BooleanOp::Not => {
            if operand_bits.len() != 1 {
                return Err(FlowGateError::BooleanNotArity(
                    gate.gate_id().clone(),
                    operand_bits.len(),
                ));
            }
            let mut acc = operand_bits.remove(0);
            for idx in 0..n_events {
                let prev = acc[idx];
                acc.set(idx, !prev);
            }
            acc
        }
    };
    Ok(out)
}

fn compute_topological_order(
    gates: &IndexMap<GateId, GateKind>,
) -> Result<Vec<GateId>, FlowGateError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Temp,
        Perm,
    }

    fn visit(
        node: &GateId,
        gates: &IndexMap<GateId, GateKind>,
        marks: &mut HashMap<GateId, Mark>,
        order: &mut Vec<GateId>,
        stack: &mut HashSet<GateId>,
    ) -> Result<(), FlowGateError> {
        if marks.get(node) == Some(&Mark::Perm) {
            return Ok(());
        }
        if marks.get(node) == Some(&Mark::Temp) || stack.contains(node) {
            return Err(FlowGateError::CyclicGateReference(node.clone()));
        }

        marks.insert(node.clone(), Mark::Temp);
        stack.insert(node.clone());

        let gate = gates
            .get(node)
            .ok_or_else(|| FlowGateError::UnknownGateReference(node.clone(), node.clone()))?;
        for dep in gate.dependency_ids() {
            if !gates.contains_key(&dep) {
                return Err(FlowGateError::UnknownGateReference(node.clone(), dep));
            }
            visit(&dep, gates, marks, order, stack)?;
        }

        marks.insert(node.clone(), Mark::Perm);
        stack.remove(node);
        order.push(node.clone());
        Ok(())
    }

    let mut marks: HashMap<GateId, Mark> = HashMap::new();
    let mut stack: HashSet<GateId> = HashSet::new();
    let mut order = Vec::with_capacity(gates.len());
    for gate_id in gates.keys() {
        visit(gate_id, gates, &mut marks, &mut order, &mut stack)?;
    }
    Ok(order)
}
