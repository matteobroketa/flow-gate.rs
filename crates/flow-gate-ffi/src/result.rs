use std::ffi::CString;
use std::os::raw::c_char;

use flow_gate_core::FlowGateError;

/// Opaque FFI error struct. The `message` field is a heap-allocated C string
/// that must be freed by calling `flow_gate_ffi_error_free`.
#[repr(C)]
pub struct FfiError {
    pub message: *mut c_char,
    pub code: u32,
}

impl FfiError {
    pub fn from_gating_error(err: &FlowGateError) -> Self {
        let msg = CString::new(err.to_string())
            .unwrap_or_else(|_| unsafe {
                // SAFETY: This literal contains no null bytes, so from_raw_unchecked is safe.
                CString::from_vec_unchecked(b"unknown error".to_vec())
            });
        Self {
            message: msg.into_raw(),
            code: error_code(err),
        }
    }
}

/// Frees the message string inside an `FfiError`.
///
/// # Safety
/// - `err` must be a valid, previously constructed `FfiError` whose `message`
///   field was produced by `CString::into_raw()`.
/// - This function must not be called more than once for the same `message`.
#[no_mangle]
pub unsafe extern "C" fn flow_gate_ffi_error_free(err: *mut FfiError) {
    if err.is_null() {
        return;
    }
    let err = &mut *err;
    if !err.message.is_null() {
        // SAFETY: message was created by CString::into_raw in FfiError::from_gating_error.
        drop(CString::from_raw(err.message));
        err.message = std::ptr::null_mut();
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
