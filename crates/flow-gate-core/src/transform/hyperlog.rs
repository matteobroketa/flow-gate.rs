use crate::error::FlowGateError;
use crate::traits::Transform;

const TAYLOR_LENGTH: usize = 16;
const LN_10: f64 = std::f64::consts::LN_10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HyperlogTransform {
    pub t: f64,
    pub w: f64,
    pub m: f64,
    pub a: f64,
    w_n: f64,
    x1: f64,
    b_coef: f64,
    a_coef: f64,
    c_coef: f64,
    f_coef: f64,
    x_taylor: f64,
    inverse_x0: f64,
    taylor: [f64; TAYLOR_LENGTH],
}

impl HyperlogTransform {
    pub fn new(t: f64, w: f64, m: f64, a: f64) -> Result<Self, FlowGateError> {
        validate_hyperlog(t, w, m, a)?;

        let w_n = w / (m + a);
        let x2 = a / (m + a);
        let x1 = x2 + w_n;
        let x0 = x2 + 2.0 * w_n;
        let b_coef = (m + a) * LN_10;

        let e2bx0 = (b_coef * x0).exp();
        let c_over_a = e2bx0 / w_n;
        let f_over_a = (b_coef * x1).exp() + c_over_a * x1;

        let a_coef = t / ((b_coef.exp() + c_over_a) - f_over_a);
        let c_coef = c_over_a * a_coef;
        let f_coef = f_over_a * a_coef;

        let x_taylor = x1 + w_n / 4.0;
        let mut taylor = [0.0_f64; TAYLOR_LENGTH];
        let mut coef = a_coef * (b_coef * x1).exp();
        for (i, entry) in taylor.iter_mut().enumerate() {
            coef *= b_coef / ((i + 1) as f64);
            *entry = coef;
        }
        taylor[0] += c_coef;

        let inverse_x0 = if x0 < x_taylor {
            taylor_series_at(&taylor, x1, x0)
        } else {
            (a_coef * (b_coef * x0).exp() + c_coef * x0) - f_coef
        };

        Ok(Self {
            t,
            w,
            m,
            a,
            w_n,
            x1,
            b_coef,
            a_coef,
            c_coef,
            f_coef,
            x_taylor,
            inverse_x0,
            taylor,
        })
    }

    #[inline]
    fn taylor_series(&self, scale: f64) -> f64 {
        taylor_series_at(&self.taylor, self.x1, scale)
    }
}

impl Transform for HyperlogTransform {
    fn apply(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return f64::NAN;
        }
        if value == 0.0 {
            return self.x1;
        }

        let negative = value < 0.0;
        let signal = value.abs();

        let mut x = if signal < self.inverse_x0 {
            self.x1 + signal * self.w_n / self.inverse_x0
        } else {
            (signal / self.a_coef).ln() / self.b_coef
        };
        if !x.is_finite() {
            return f64::NAN;
        }

        let tolerance = if x > 1.0 {
            3.0 * x * f64::EPSILON
        } else {
            3.0 * f64::EPSILON
        };

        for _ in 0..10 {
            let ae2bx = self.a_coef * (self.b_coef * x).exp();
            let y = if x < self.x_taylor {
                self.taylor_series(x) - signal
            } else {
                (ae2bx + self.c_coef * x) - (self.f_coef + signal)
            };

            let abe2bx = self.b_coef * ae2bx;
            let dy = abe2bx + self.c_coef;
            let ddy = self.b_coef * abe2bx;
            let denom = dy * (1.0 - y * ddy / (2.0 * dy * dy));
            if !denom.is_finite() || denom.abs() < f64::EPSILON {
                return f64::NAN;
            }
            let delta = y / denom;
            if !delta.is_finite() {
                return f64::NAN;
            }
            x -= delta;
            if delta.abs() < tolerance {
                return if negative { 2.0 * self.x1 - x } else { x };
            }
        }
        f64::NAN
    }

    fn invert(&self, scaled: f64) -> f64 {
        if !scaled.is_finite() {
            return f64::NAN;
        }

        let negative = scaled < self.x1;
        let reflected = if negative {
            2.0 * self.x1 - scaled
        } else {
            scaled
        };

        let inverse = if reflected < self.x_taylor {
            self.taylor_series(reflected)
        } else {
            (self.a_coef * (self.b_coef * reflected).exp() + self.c_coef * reflected) - self.f_coef
        };

        if !inverse.is_finite() {
            return f64::NAN;
        }
        if negative {
            -inverse
        } else {
            inverse
        }
    }

    fn transform_id(&self) -> &str {
        "hyperlog"
    }
}

#[inline]
fn taylor_series_at(taylor: &[f64; TAYLOR_LENGTH], x1: f64, scale: f64) -> f64 {
    let x = scale - x1;
    let mut sum = taylor[TAYLOR_LENGTH - 1] * x;
    for i in (0..(TAYLOR_LENGTH - 1)).rev() {
        sum = (sum + taylor[i]) * x;
    }
    sum
}

fn validate_hyperlog(t: f64, w: f64, m: f64, a: f64) -> Result<(), FlowGateError> {
    if !t.is_finite() || t <= 0.0 {
        return Err(FlowGateError::InvalidTransformParam(
            "Hyperlog parameter T must be finite and > 0".to_string(),
        ));
    }
    if !m.is_finite() || m <= 0.0 {
        return Err(FlowGateError::InvalidTransformParam(
            "Hyperlog parameter M must be finite and > 0".to_string(),
        ));
    }
    if !w.is_finite() || w <= 0.0 {
        return Err(FlowGateError::InvalidTransformParam(
            "Hyperlog parameter W must be finite and > 0".to_string(),
        ));
    }
    if !a.is_finite() {
        return Err(FlowGateError::InvalidTransformParam(
            "Hyperlog parameter A must be finite".to_string(),
        ));
    }
    if 2.0 * w > m {
        return Err(FlowGateError::InvalidTransformParam(
            "Hyperlog constraint violated: 2W <= M".to_string(),
        ));
    }
    if -a > w || (a + w) > (m - w) {
        return Err(FlowGateError::InvalidTransformParam(
            "Hyperlog parameters violate A/W/M constraints".to_string(),
        ));
    }
    Ok(())
}
