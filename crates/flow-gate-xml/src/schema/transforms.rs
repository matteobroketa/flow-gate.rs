use flow_gate_core::TransformKind;

#[derive(Debug, Clone, PartialEq)]
pub struct NamedTransform {
    pub id: String,
    pub transform: TransformKind,
}
