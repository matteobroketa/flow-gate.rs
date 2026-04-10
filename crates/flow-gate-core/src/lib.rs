pub mod error;
pub mod event;
pub mod gate;
pub mod traits;
pub mod transform;

pub use error::FlowGateError;
pub use event::{
    ColumnIter, EventMatrix, EventMatrixView, MatrixLayout, MatrixView, ProjectedMatrix,
};
pub use gate::{
    BooleanGate, BooleanOp, BooleanOperand, EllipsoidCovariance, EllipsoidDimension, EllipsoidGate,
    GateKind, GateRegistry, PolygonDimension, PolygonGate, RectangleDimension, RectangleGate,
};
pub use traits::{ApplyGate, BitVec, Gate, GateId, ParameterName, Transform};
pub use transform::{
    FASinhTransform, HyperlogTransform, LinearTransform, LogarithmicTransform, LogicleParams,
    LogicleTransform, TransformKind,
};
