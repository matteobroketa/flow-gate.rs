use crate::error::FlowGateError;
use crate::traits::{Gate, GateId, ParameterName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    And,
    Or,
    Not,
}

#[derive(Debug, Clone)]
pub struct BooleanOperand {
    pub gate_id: GateId,
    pub complement: bool,
}

#[derive(Debug, Clone)]
pub struct BooleanGate {
    id: GateId,
    parent_id: Option<GateId>,
    op: BooleanOp,
    operands: Vec<BooleanOperand>,
    dim_names: Vec<ParameterName>,
}

impl BooleanGate {
    pub fn new(
        id: GateId,
        parent_id: Option<GateId>,
        op: BooleanOp,
        operands: Vec<BooleanOperand>,
    ) -> Result<Self, FlowGateError> {
        if operands.is_empty() {
            return Err(FlowGateError::BooleanEmptyOperands(id));
        }
        if op == BooleanOp::Not && operands.len() != 1 {
            return Err(FlowGateError::BooleanNotArity(id, operands.len()));
        }
        Ok(Self {
            id,
            parent_id,
            op,
            operands,
            dim_names: Vec::new(),
        })
    }

    pub fn op(&self) -> BooleanOp {
        self.op
    }

    pub fn operands(&self) -> &[BooleanOperand] {
        &self.operands
    }
}

impl Gate for BooleanGate {
    fn dimensions(&self) -> &[ParameterName] {
        &self.dim_names
    }

    fn contains(&self, _coords: &[f64]) -> bool {
        false
    }

    fn gate_id(&self) -> &GateId {
        &self.id
    }

    fn parent_id(&self) -> Option<&GateId> {
        self.parent_id.as_ref()
    }
}
