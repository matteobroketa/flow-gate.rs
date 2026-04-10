pub mod matrix;
pub mod result;

pub use matrix::{ColumnIter, MatrixLayout, MatrixView};
pub use result::{flow_gate_ffi_error_free, FfiError};
