use crate::error::FlowGateError;
use crate::traits::Transform;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogarithmicTransform {
    pub t: f64,
    pub m: f64,
    log10_t: f64,
    inv_m: f64,
}

impl LogarithmicTransform {
    pub fn new(t: f64, m: f64) -> Result<Self, FlowGateError> {
        if !t.is_finite() || t <= 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "Logarithmic parameter T must be finite and > 0".to_string(),
            ));
        }
        if !m.is_finite() || m <= 0.0 {
            return Err(FlowGateError::InvalidTransformParam(
                "Logarithmic parameter M must be finite and > 0".to_string(),
            ));
        }
        Ok(Self {
            t,
            m,
            log10_t: t.log10(),
            inv_m: 1.0 / m,
        })
    }
}

impl Transform for LogarithmicTransform {
    fn apply(&self, value: f64) -> f64 {
        if !value.is_finite() || value <= 0.0 {
            return f64::NAN;
        }
        (value.log10() - self.log10_t) * self.inv_m + 1.0
    }

    fn invert(&self, scaled: f64) -> f64 {
        if !scaled.is_finite() {
            return f64::NAN;
        }
        let value = self.t * 10.0_f64.powf((scaled - 1.0) * self.m);
        if value.is_finite() {
            value
        } else {
            f64::NAN
        }
    }

    fn transform_id(&self) -> &str {
        "log"
    }
}
