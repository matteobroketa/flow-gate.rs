use crate::error::FlowGateError;
use crate::traits::Transform;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearTransform {
    pub t: f64,
    pub a: f64,
    span: f64,
    inv_span: f64,
}

impl LinearTransform {
    pub fn new(t: f64, a: f64) -> Result<Self, FlowGateError> {
        if !t.is_finite() || t <= 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "Linear parameter T must be finite and > 0".to_string(),
            ));
        }
        if !a.is_finite() {
            return Err(FlowGateError::InvalidTransformParam(
                "Linear parameter A must be finite".to_string(),
            ));
        }
        let span = t + a;
        if !span.is_finite() || span <= 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "Linear span (T + A) must be finite and > 0".to_string(),
            ));
        }
        if span.is_subnormal() {
            return Err(FlowGateError::InvalidTransformParam(
                "Linear span (T + A) is subnormal".to_string(),
            ));
        }
        Ok(Self {
            t,
            a,
            span,
            inv_span: 1.0 / span,
        })
    }
}

impl Transform for LinearTransform {
    fn apply(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return f64::NAN;
        }
        (value + self.a) * self.inv_span
    }

    fn invert(&self, scaled: f64) -> f64 {
        if !scaled.is_finite() {
            return f64::NAN;
        }
        let value = scaled * self.span - self.a;
        if value.is_finite() {
            value
        } else {
            f64::NAN
        }
    }

    fn transform_id(&self) -> &str {
        "lin"
    }
}
