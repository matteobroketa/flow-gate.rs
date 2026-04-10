use flow_gate_core::{gate::GateKind, GateId};

#[derive(Debug, Clone)]
pub struct NamedGate {
    pub id: GateId,
    pub gate: GateKind,
}
