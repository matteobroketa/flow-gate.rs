use crate::traits::{GateId, ParameterName};

#[derive(Debug, Clone, thiserror::Error)]
pub enum FlowGateError {
    #[error("Transform parameter error: {0}")]
    InvalidTransformParam(String),

    #[error("Gate definition is invalid: {0}")]
    InvalidGate(String),

    #[error("Unknown parameter '{0}' in EventMatrix")]
    UnknownParameter(ParameterName),

    #[error("Gate '{0}' references unknown gate '{1}'")]
    UnknownGateReference(GateId, GateId),

    #[error("Gate hierarchy contains a cycle involving gate '{0}'")]
    CyclicGateReference(GateId),

    #[error("Missing parent gate '{0}' in classification results")]
    MissingParentGate(GateId),

    #[error("Covariance matrix is not positive-definite")]
    NotPositiveDefinite,

    #[error("EllipsoidGate dimension mismatch: mean has {0} elements, matrix has {1}×{1}")]
    DimensionMismatch(usize, usize),

    #[error("XML parse error: {0}")]
    XmlParse(String),

    #[error("Missing required XML attribute '{0}' on element '{1}'")]
    MissingAttribute(String, String),

    #[error("Invalid f64 value '{0}' for attribute '{1}'")]
    InvalidFloat(String, String),

    #[error("BooleanGate '{0}' has NOT operator with {1} operands (expected 1)")]
    BooleanNotArity(GateId, usize),

    #[error("BooleanGate '{0}' has no operands")]
    BooleanEmptyOperands(GateId),
}
