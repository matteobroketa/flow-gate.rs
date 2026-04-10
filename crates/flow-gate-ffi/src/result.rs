use std::ffi::CString;
use std::os::raw::c_char;

use flow_gate_core::FlowGateError;

#[repr(C)]
pub enum FfiResult<T> {
    Ok(T),
    Err(FfiError),
}

#[repr(C)]
pub struct FfiError {
    pub message: *mut c_char,
    pub code: u32,
}

impl FfiError {
    pub fn from_gating_error(err: &FlowGateError) -> Self {
        let msg = CString::new(err.to_string())
            .unwrap_or_else(|_| CString::new("unknown error").expect("literal"));
        Self {
            message: msg.into_raw(),
            code: error_code(err),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn ffi_error_free(err: FfiError) {
    if !err.message.is_null() {
        drop(CString::from_raw(err.message));
    }
}

fn error_code(err: &FlowGateError) -> u32 {
    match err {
        FlowGateError::InvalidTransformParam(_) => 1,
        FlowGateError::InvalidGate(_) => 2,
        FlowGateError::UnknownParameter(_) => 3,
        FlowGateError::XmlParse(_) => 4,
        FlowGateError::NotPositiveDefinite => 5,
        FlowGateError::CyclicGateReference(_) => 6,
        FlowGateError::UnknownGateReference(_, _) => 7,
        FlowGateError::MissingAttribute(_, _) => 8,
        FlowGateError::InvalidFloat(_, _) => 9,
        FlowGateError::BooleanNotArity(_, _) => 10,
        FlowGateError::BooleanEmptyOperands(_) => 11,
        FlowGateError::DimensionMismatch(_, _) => 12,
        FlowGateError::MissingParentGate(_) => 13,
    }
}
