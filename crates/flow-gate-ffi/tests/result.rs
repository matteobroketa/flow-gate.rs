use flow_gate_core::{GateId, FlowGateError, ParameterName};
use flow_gate_ffi::{ffi_error_free, FfiError};

#[test]
fn ffi_error_codes_are_stable() {
    let samples = [
        (FlowGateError::InvalidTransformParam("x".into()), 1_u32),
        (FlowGateError::InvalidGate("x".into()), 2_u32),
        (
            FlowGateError::UnknownParameter(ParameterName::from("P1")),
            3_u32,
        ),
        (FlowGateError::XmlParse("x".into()), 4_u32),
        (FlowGateError::NotPositiveDefinite, 5_u32),
        (FlowGateError::CyclicGateReference(GateId::from("G1")), 6_u32),
        (
            FlowGateError::UnknownGateReference(GateId::from("G1"), GateId::from("G9")),
            7_u32,
        ),
        (
            FlowGateError::MissingAttribute("id".into(), "Gate".into()),
            8_u32,
        ),
        (FlowGateError::InvalidFloat("x".into(), "value".into()), 9_u32),
        (FlowGateError::BooleanNotArity(GateId::from("G2"), 2), 10_u32),
        (
            FlowGateError::BooleanEmptyOperands(GateId::from("G3")),
            11_u32,
        ),
        (FlowGateError::DimensionMismatch(1, 2), 12_u32),
        (FlowGateError::MissingParentGate(GateId::from("P")), 13_u32),
    ];

    for (err, expected_code) in samples {
        let ffi = FfiError::from_gating_error(&err);
        assert_eq!(ffi.code, expected_code);
        // SAFETY: message pointer was allocated by FfiError::from_gating_error.
        unsafe { ffi_error_free(ffi) };
    }
}
