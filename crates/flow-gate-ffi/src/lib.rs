pub mod matrix;
pub mod result;

pub use matrix::{ColumnIter, MatrixLayout, MatrixView};
pub use result::{ffi_error_free, FfiError, FfiResult};
