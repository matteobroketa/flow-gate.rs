use rayon::prelude::*;

pub const LOGICLE_IMPL_VERSION: &str = "logicle_v1";

const TAYLOR_LENGTH: usize = 16;
const DEFAULT_FORWARD_LUT_BINS: usize = 16_384;
const DEFAULT_INVERSE_LUT_BINS: usize = 8_192;
const MAX_FORWARD_LUT_BINS: usize = 32_768;
const LUT_ERROR_THRESHOLD: f64 = 1e-4;
const MIN_SPAN: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicleParams {
    pub t: f64,
    pub w: f64,
    pub m: f64,
    pub a: f64,
}

impl Default for LogicleParams {
    fn default() -> Self {
        Self {
            t: 262_144.0,
            w: 0.5,
            m: 4.5,
            a: 0.0,
        }
    }
}

impl LogicleParams {
    pub fn validate(self) -> Result<Self, String> {
        if !self.t.is_finite() || self.t <= 0.0 {
            return Err("Logicle parameter T must be finite and > 0".to_string());
        }
        if !self.m.is_finite() || self.m <= 0.0 {
            return Err("Logicle parameter M must be finite and > 0".to_string());
        }
        if !self.w.is_finite() || self.w < 0.0 {
            return Err("Logicle parameter W must be finite and >= 0".to_string());
        }
        if !self.a.is_finite() {
            return Err("Logicle parameter A must be finite".to_string());
        }
        if 2.0 * self.w > self.m {
            return Err("Logicle parameter W is too large for M".to_string());
        }
        if -self.a > self.w || (self.a + self.w) > (self.m - self.w) {
            return Err("Logicle parameters violate A/W/M constraints".to_string());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxisTickKind {
    Major,
    Minor,
    Zero,
}

#[derive(Debug, Clone)]
pub struct AxisTick {
    pub signal_value: f64,
    pub transformed_position: f64,
    pub kind: AxisTickKind,
}

#[derive(Debug, Clone)]
struct LogicleContext {
    params: LogicleParams,
    // Internal coefficients for the modified biexponential:
    // B(y) = a*exp(b*y) - c*exp(-d*y) - f
    a_coef: f64,
    b_coef: f64,
    c_coef: f64,
    d_coef: f64,
    f_coef: f64,
    x0: f64,
    x1: f64,
    x_taylor: f64,
    taylor: [f64; TAYLOR_LENGTH],
}

impl LogicleContext {
    fn new(params: LogicleParams) -> Result<Self, String> {
        let params = params.validate()?;

        // Parks/Moore parameterization in Gating-ML / flowCore implementations.
        let w = params.w / (params.m + params.a);
        let x2 = params.a / (params.m + params.a);
        let x1 = x2 + w;
        let x0 = x2 + 2.0 * w;

        let b_coef = (params.m + params.a) * std::f64::consts::LN_10;
        let d_coef = solve_d(b_coef, w)?;
        let c_over_a = (x0 * (b_coef + d_coef)).exp();
        let m_over_a = (b_coef * x1).exp() - c_over_a / (d_coef * x1).exp();
        let a_coef = params.t / (((b_coef).exp() - m_over_a) - c_over_a / (d_coef).exp());
        let c_coef = c_over_a * a_coef;
        let f_coef = -m_over_a * a_coef;

        let x_taylor = x1 + w / 4.0;
        let mut taylor = [0.0_f64; TAYLOR_LENGTH];
        let mut pos_coef = a_coef * (b_coef * x1).exp();
        let mut neg_coef = -c_coef / (d_coef * x1).exp();
        for (i, slot) in taylor.iter_mut().enumerate() {
            pos_coef *= b_coef / (i as f64 + 1.0);
            neg_coef *= -d_coef / (i as f64 + 1.0);
            *slot = pos_coef + neg_coef;
        }
        // By construction this should be exactly zero in the ideal math.
        taylor[1] = 0.0;

        Ok(Self {
            params,
            a_coef,
            b_coef,
            c_coef,
            d_coef,
            f_coef,
            x0,
            x1,
            x_taylor,
            taylor,
        })
    }

    fn params(&self) -> LogicleParams {
        self.params
    }

    fn series_biexponential(&self, scale: f64) -> f64 {
        // Taylor expansion around x1 to stabilize near zero.
        let x = scale - self.x1;
        let mut sum = self.taylor[TAYLOR_LENGTH - 1] * x;
        for i in (2..=(TAYLOR_LENGTH - 2)).rev() {
            sum = (sum + self.taylor[i]) * x;
        }
        (sum * x + self.taylor[0]) * x
    }

    fn inverse_positive(&self, scale: f64) -> f64 {
        if scale < self.x_taylor {
            self.series_biexponential(scale)
        } else {
            (self.a_coef * (self.b_coef * scale).exp() + self.f_coef)
                - self.c_coef / (self.d_coef * scale).exp()
        }
    }

    fn eval_forward_function(&self, scale: f64, signal: f64) -> f64 {
        self.inverse_positive(scale) - signal
    }

    fn forward_positive_halley(&self, signal: f64) -> Option<f64> {
        let mut x = if signal < self.f_coef {
            self.x1 + signal / self.taylor[0]
        } else {
            (signal / self.a_coef).ln() / self.b_coef
        };

        let mut tolerance = 3.0 * f64::EPSILON;
        if x > 1.0 {
            tolerance = 3.0 * x * f64::EPSILON;
        }

        for _ in 0..16 {
            let ae2bx = self.a_coef * (self.b_coef * x).exp();
            let ce2mdx = self.c_coef / (self.d_coef * x).exp();
            let y = if x < self.x_taylor {
                self.series_biexponential(x) - signal
            } else {
                (ae2bx + self.f_coef) - (ce2mdx + signal)
            };
            let dy = self.b_coef * ae2bx + self.d_coef * ce2mdx;
            if !dy.is_finite() || dy.abs() < f64::EPSILON {
                return None;
            }
            let ddy = self.b_coef * self.b_coef * ae2bx - self.d_coef * self.d_coef * ce2mdx;
            let denom = dy * (1.0 - y * ddy / (2.0 * dy * dy));
            if !denom.is_finite() || denom.abs() < f64::EPSILON {
                return None;
            }
            let delta = y / denom;
            if !delta.is_finite() {
                return None;
            }
            x -= delta;
            if delta.abs() < tolerance {
                return Some(x);
            }
        }
        None
    }

    fn forward_positive_bisection(&self, signal: f64) -> Option<f64> {
        // Monotonic root solve fallback for robustness.
        let mut lo = self.x1;
        let mut hi = (self.x1 + 1.0).max(1.0);

        let mut f_lo = self.eval_forward_function(lo, signal);
        if !f_lo.is_finite() {
            return None;
        }
        if f_lo >= 0.0 {
            return Some(lo);
        }

        let mut f_hi = self.eval_forward_function(hi, signal);
        let mut guard = 0;
        while (!f_hi.is_finite() || f_hi < 0.0) && guard < 64 {
            let span = (hi - lo).max(0.5);
            hi += span * 1.8;
            f_hi = self.eval_forward_function(hi, signal);
            guard += 1;
        }
        if !f_hi.is_finite() || f_hi < 0.0 {
            return None;
        }

        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            let f_mid = self.eval_forward_function(mid, signal);
            if !f_mid.is_finite() {
                return None;
            }
            if f_mid.abs() < 1e-12 || (hi - lo).abs() < 1e-12 {
                return Some(mid);
            }
            if f_mid < 0.0 {
                lo = mid;
                f_lo = f_mid;
            } else {
                hi = mid;
                f_hi = f_mid;
            }
            if (f_hi - f_lo).abs() < 1e-20 {
                break;
            }
        }
        Some(0.5 * (lo + hi))
    }

    fn forward_positive(&self, signal: f64) -> f64 {
        if signal == 0.0 {
            return self.x1;
        }
        if let Some(value) = self.forward_positive_halley(signal) {
            return value;
        }
        self.forward_positive_bisection(signal).unwrap_or(self.x1)
    }

    fn forward(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return f64::NAN;
        }
        if value == 0.0 {
            return self.x1;
        }
        if value < 0.0 {
            let pos = self.forward_positive(-value);
            return 2.0 * self.x1 - pos;
        }
        self.forward_positive(value)
    }

    fn inverse(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return f64::NAN;
        }
        let negative = value < self.x1;
        let reflected = if negative {
            2.0 * self.x1 - value
        } else {
            value
        };
        let inverse = self.inverse_positive(reflected);
        if negative {
            -inverse
        } else {
            inverse
        }
    }

    fn linear_half_signal(&self) -> f64 {
        self.inverse(self.x0).abs()
    }
}

fn solve_d(b: f64, w: f64) -> Result<f64, String> {
    // Root solve for d from Moore/Parks biexponential constraints.
    if w == 0.0 {
        return Ok(b);
    }
    let tolerance = 2.0 * b * f64::EPSILON;
    let mut d_lo = 0.0_f64;
    let mut d_hi = b;
    let mut d = 0.5 * (d_lo + d_hi);
    let mut last_delta = d_hi - d_lo;
    let f_b = -2.0 * b.ln() + w * b;
    let mut f = 2.0 * d.ln() + w * d + f_b;
    let mut last_f = f64::NAN;

    for _ in 0..40 {
        let df = 2.0 / d + w;
        let newton_unsafe = ((d - d_hi) * df - f) * ((d - d_lo) * df - f) >= 0.0
            || (1.9 * f).abs() > (last_delta * df).abs();

        let delta = if newton_unsafe {
            let delta = 0.5 * (d_hi - d_lo);
            d = d_lo + delta;
            delta
        } else {
            let delta = f / df;
            d -= delta;
            delta
        };

        if !delta.is_finite() {
            return Err("Logicle d solve failed: non-finite update".to_string());
        }
        if delta.abs() < tolerance {
            return Ok(d);
        }
        last_delta = delta;

        f = 2.0 * d.ln() + w * d + f_b;
        if f == 0.0 || f == last_f {
            return Ok(d);
        }
        last_f = f;

        if f < 0.0 {
            d_lo = d;
        } else {
            d_hi = d;
        }
    }

    Err("Logicle d solve failed to converge".to_string())
}

#[derive(Debug, Clone)]
pub struct LogicleLut {
    context: LogicleContext,
    signal_min: f64,
    signal_max: f64,
    signal_step: f64,
    forward_lut: Vec<f32>,
    domain_min: f64,
    domain_max: f64,
    domain_step: f64,
    inverse_lut: Vec<f32>,
}

impl LogicleLut {
    pub fn build(
        params: LogicleParams,
        signal_min: f64,
        signal_max: f64,
        forward_bins: usize,
        inverse_bins: usize,
    ) -> Result<Self, String> {
        let context = LogicleContext::new(params)?;
        let lo = signal_min.min(signal_max);
        let mut hi = signal_min.max(signal_max);
        if (hi - lo).abs() < MIN_SPAN {
            hi = lo + MIN_SPAN;
        }

        let forward_bins = forward_bins.max(2);
        let signal_step = (hi - lo) / (forward_bins as f64 - 1.0);
        let mut forward_lut = vec![0.0_f32; forward_bins];
        for (i, slot) in forward_lut.iter_mut().enumerate() {
            let signal = lo + signal_step * i as f64;
            *slot = context.forward(signal) as f32;
        }

        let mut domain_min = forward_lut.first().copied().unwrap_or(0.0) as f64;
        let mut domain_max = forward_lut.last().copied().unwrap_or(1.0) as f64;
        if !domain_min.is_finite()
            || !domain_max.is_finite()
            || (domain_max - domain_min).abs() < MIN_SPAN
        {
            domain_min = context.forward(lo);
            domain_max = context.forward(hi);
            if (domain_max - domain_min).abs() < MIN_SPAN {
                domain_max = domain_min + MIN_SPAN;
            }
        }

        let inverse_bins = inverse_bins.max(2);
        let domain_step = (domain_max - domain_min) / (inverse_bins as f64 - 1.0);
        let mut inverse_lut = vec![0.0_f32; inverse_bins];
        for (i, slot) in inverse_lut.iter_mut().enumerate() {
            let domain = domain_min + domain_step * i as f64;
            *slot = context.inverse(domain) as f32;
        }

        Ok(Self {
            context,
            signal_min: lo,
            signal_max: hi,
            signal_step,
            forward_lut,
            domain_min,
            domain_max,
            domain_step,
            inverse_lut,
        })
    }

    pub fn build_adaptive(
        params: LogicleParams,
        signal_min: f64,
        signal_max: f64,
    ) -> Result<Self, String> {
        let mut lut = Self::build(
            params,
            signal_min,
            signal_max,
            DEFAULT_FORWARD_LUT_BINS,
            DEFAULT_INVERSE_LUT_BINS,
        )?;
        if lut.max_forward_abs_error_sampled(256) > LUT_ERROR_THRESHOLD {
            lut = Self::build(
                params,
                signal_min,
                signal_max,
                MAX_FORWARD_LUT_BINS,
                DEFAULT_INVERSE_LUT_BINS,
            )?;
        }
        Ok(lut)
    }

    pub fn params(&self) -> LogicleParams {
        self.context.params()
    }

    pub fn forward(&self, signal: f64) -> f64 {
        if !signal.is_finite() {
            return f64::NAN;
        }
        if signal < self.signal_min || signal > self.signal_max {
            return self.context.forward(signal);
        }
        let idx_f = (signal - self.signal_min) / self.signal_step;
        interpolate_lut(idx_f, &self.forward_lut)
    }

    pub fn inverse(&self, domain: f64) -> f64 {
        if !domain.is_finite() {
            return f64::NAN;
        }
        if domain < self.domain_min || domain > self.domain_max {
            return self.context.inverse(domain);
        }
        let idx_f = (domain - self.domain_min) / self.domain_step;
        interpolate_lut(idx_f, &self.inverse_lut)
    }

    pub fn max_forward_abs_error_sampled(&self, samples: usize) -> f64 {
        let sample_count = samples.max(16);
        let span = (self.signal_max - self.signal_min).max(MIN_SPAN);
        let mut max_err = 0.0_f64;
        for i in 0..sample_count {
            let signal = self.signal_min + span * (i as f64 / (sample_count - 1) as f64);
            let direct = self.context.forward(signal);
            let approx = self.forward(signal);
            let err = (direct - approx).abs();
            if err > max_err {
                max_err = err;
            }
        }
        max_err
    }
}

fn interpolate_lut(index: f64, lut: &[f32]) -> f64 {
    if lut.is_empty() || !index.is_finite() {
        return f64::NAN;
    }
    if lut.len() == 1 {
        return lut[0] as f64;
    }
    let low = index.floor().clamp(0.0, (lut.len() - 1) as f64) as usize;
    let high = (low + 1).min(lut.len() - 1);
    let frac = (index - low as f64).clamp(0.0, 1.0);
    let a = lut[low] as f64;
    let b = lut[high] as f64;
    a + (b - a) * frac
}

pub fn logicle_forward(value: f64, params: LogicleParams) -> f64 {
    match LogicleContext::new(params) {
        Ok(ctx) => ctx.forward(value),
        Err(_) => f64::NAN,
    }
}

pub fn logicle_inverse(value: f64, params: LogicleParams) -> f64 {
    match LogicleContext::new(params) {
        Ok(ctx) => ctx.inverse(value),
        Err(_) => f64::NAN,
    }
}

pub fn apply_transform_logicle(values: &[f32], params: LogicleParams, out: &mut [f32]) {
    if values.len() != out.len() {
        return;
    }
    if values.is_empty() {
        return;
    }

    let mut min_signal = f64::INFINITY;
    let mut max_signal = f64::NEG_INFINITY;
    for value in values.iter().copied() {
        let v = value as f64;
        if !v.is_finite() {
            continue;
        }
        min_signal = min_signal.min(v);
        max_signal = max_signal.max(v);
    }
    if !min_signal.is_finite() || !max_signal.is_finite() {
        out.fill(f32::NAN);
        return;
    }

    let lut = match LogicleLut::build_adaptive(params, min_signal, max_signal) {
        Ok(lut) => lut,
        Err(_) => {
            out.fill(f32::NAN);
            return;
        }
    };

    out.par_iter_mut()
        .zip(values.par_iter())
        .for_each(|(dst, src)| {
            let value = *src as f64;
            *dst = if value.is_finite() {
                lut.forward(value) as f32
            } else {
                f32::NAN
            };
        });
}

fn nice_linear_step(span: f64, target_ticks: usize) -> f64 {
    let target = target_ticks.max(2) as f64;
    let rough = (span / target).abs().max(MIN_SPAN);
    let mag = 10.0_f64.powf(rough.log10().floor());
    let residual = rough / mag;
    let nice = if residual <= 1.0 {
        1.0
    } else if residual <= 2.0 {
        2.0
    } else if residual <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn add_log_decade_ticks(
    min_signal: f64,
    max_signal: f64,
    sign: f64,
    out: &mut Vec<(f64, AxisTickKind)>,
) {
    if min_signal <= 0.0 || max_signal <= 0.0 || max_signal < min_signal {
        return;
    }
    let start_pow = min_signal.log10().floor() as i32;
    let end_pow = max_signal.log10().ceil() as i32;
    let range_lo = if sign >= 0.0 { min_signal } else { -max_signal };
    let range_hi = if sign >= 0.0 { max_signal } else { -min_signal };
    for p in start_pow..=end_pow {
        let base = 10.0_f64.powi(p);
        for (mult, kind) in [
            (1.0_f64, AxisTickKind::Major),
            (2.0_f64, AxisTickKind::Minor),
            (5.0_f64, AxisTickKind::Minor),
        ] {
            let candidate = sign * base * mult;
            if candidate >= range_lo && candidate <= range_hi {
                out.push((candidate, kind));
            }
        }
    }
}

pub fn logicle_axis_ticks(
    params: LogicleParams,
    domain_min: f64,
    domain_max: f64,
) -> Result<Vec<AxisTick>, String> {
    let ctx = LogicleContext::new(params)?;
    let d_lo = domain_min.min(domain_max);
    let d_hi = domain_min.max(domain_max);
    if !d_lo.is_finite() || !d_hi.is_finite() || (d_hi - d_lo).abs() < MIN_SPAN {
        return Ok(Vec::new());
    }

    let signal_a = ctx.inverse(d_lo);
    let signal_b = ctx.inverse(d_hi);
    if !signal_a.is_finite() || !signal_b.is_finite() {
        return Ok(Vec::new());
    }
    let s_lo = signal_a.min(signal_b);
    let s_hi = signal_a.max(signal_b);

    let mut candidates: Vec<(f64, AxisTickKind)> = Vec::new();

    if s_lo <= 0.0 && s_hi >= 0.0 {
        candidates.push((0.0, AxisTickKind::Zero));
    }
    if s_hi > 0.0 {
        add_log_decade_ticks(s_lo.max(MIN_SPAN), s_hi, 1.0, &mut candidates);
    }
    if s_lo < 0.0 {
        add_log_decade_ticks((-s_hi).max(MIN_SPAN), -s_lo, -1.0, &mut candidates);
    }

    // Ensure linear neighborhood around zero has readable ticks.
    let linear_half = ctx.linear_half_signal().max(1.0);
    let linear_extent = linear_half.min(s_hi.abs().max(s_lo.abs()));
    let linear_step = nice_linear_step(2.0 * linear_extent, 8);
    for i in -4..=4 {
        let signal = i as f64 * linear_step;
        if signal >= s_lo && signal <= s_hi {
            let kind = if signal == 0.0 {
                AxisTickKind::Zero
            } else {
                AxisTickKind::Minor
            };
            candidates.push((signal, kind));
        }
    }

    let mut ticks = Vec::new();
    for (signal, kind) in candidates {
        let transformed = ctx.forward(signal);
        if !transformed.is_finite() {
            continue;
        }
        if transformed < d_lo - 1e-9 || transformed > d_hi + 1e-9 {
            continue;
        }
        ticks.push(AxisTick {
            signal_value: signal,
            transformed_position: transformed,
            kind,
        });
    }

    ticks.sort_by(|a, b| {
        let by_pos = a
            .transformed_position
            .partial_cmp(&b.transformed_position)
            .unwrap_or(std::cmp::Ordering::Equal);
        if by_pos != std::cmp::Ordering::Equal {
            return by_pos;
        }
        axis_kind_priority(&a.kind).cmp(&axis_kind_priority(&b.kind))
    });
    ticks.dedup_by(|a, b| {
        (a.transformed_position - b.transformed_position).abs() < 1e-7
            || (a.signal_value - b.signal_value).abs() < 1e-7
    });

    Ok(ticks)
}

fn axis_kind_priority(kind: &AxisTickKind) -> i32 {
    match kind {
        AxisTickKind::Zero => 0,
        AxisTickKind::Major => 1,
        AxisTickKind::Minor => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_forward_inverse_is_stable() {
        let p = LogicleParams::default();
        let values = [
            -10_000.0, -2_000.0, -250.0, -5.0, 0.0, 5.0, 250.0, 5_000.0, 50_000.0, 250_000.0,
        ];
        for x in values {
            let y = logicle_forward(x, p);
            let x2 = logicle_inverse(y, p);
            let abs_err = (x - x2).abs();
            let rel = abs_err / x.abs().max(1.0);
            assert!(rel < 1e-6, "round-trip error too high at {x}: got {x2}");
        }
    }

    #[test]
    fn forward_is_monotonic() {
        let ctx = LogicleContext::new(LogicleParams::default()).expect("context");
        let mut last = f64::NEG_INFINITY;
        for i in -10_000..=10_000 {
            let signal = i as f64 * 30.0;
            let y = ctx.forward(signal);
            assert!(y >= last, "non-monotonic at {signal}: {y} < {last}");
            last = y;
        }
    }

    #[test]
    fn lut_error_is_small() {
        let params = LogicleParams::default();
        let lut = LogicleLut::build_adaptive(params, -25_000.0, 262_144.0).expect("lut");
        assert!(lut.max_forward_abs_error_sampled(512) < 1e-3);
    }
}
