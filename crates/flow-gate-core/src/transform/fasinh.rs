use crate::error::FlowGateError;
use crate::traits::Transform;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FASinhTransform {
    pub t: f64,
    pub m: f64,
    pub a: f64,
    scale: f64,
    offset: f64,
    divisor: f64,
    sinh_m: f64,
}

impl FASinhTransform {
    pub fn new(t: f64, m: f64, a: f64) -> Result<Self, FlowGateError> {
        if !t.is_finite() || t <= 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "FASinh parameter T must be finite and > 0".to_string(),
            ));
        }
        if !m.is_finite() || m <= 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "FASinh parameter M must be finite and > 0".to_string(),
            ));
        }
        if !a.is_finite() || a < 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "FASinh parameter A must be finite and >= 0".to_string(),
            ));
        }
        let ln10 = 10.0_f64.ln();
        let sinh_m = (m * ln10).sinh();
        let divisor = (m + a) * ln10;
        if divisor <= 0.0 || !divisor.is_finite() {
            return Err(FlowGateError::InvalidTransformParam(
                "FASinh divisor is non-positive or non-finite".to_string(),
            ));
        }
        let scale = sinh_m / t;
        let offset = a * ln10;
        Ok(Self {
            t,
            m,
            a,
            scale,
            offset,
            divisor,
            sinh_m,
        })
    }
}

impl Transform for FASinhTransform {
    fn apply(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return f64::NAN;
        }
        ((value * self.scale).asinh() + self.offset) / self.divisor
    }

    fn invert(&self, scaled: f64) -> f64 {
        if !scaled.is_finite() {
            return f64::NAN;
        }
        let x = self.t * (scaled * self.divisor - self.offset).sinh() / self.sinh_m;
        if x.is_finite() {
            x
        } else {
            f64::NAN
        }
    }

    fn transform_id(&self) -> &str {
        "fasinh"
    }
}
