#[path = "logicle_impl.rs"]
mod logicle_impl;
pub use logicle_impl::{
    apply_transform_logicle, logicle_forward, logicle_inverse, LogicleLut, LogicleParams,
};

use crate::traits::Transform;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicleTransform {
    pub params: LogicleParams,
}

impl Transform for LogicleTransform {
    fn apply(&self, value: f64) -> f64 {
        logicle_forward(value, self.params)
    }

    fn invert(&self, scaled: f64) -> f64 {
        logicle_inverse(scaled, self.params)
    }

    fn apply_batch(&self, values: &[f64], out: &mut [f64]) {
        debug_assert_eq!(values.len(), out.len());
        if values.len() < 1024 {
            for (&value, dst) in values.iter().zip(out.iter_mut()) {
                *dst = self.apply(value);
            }
            return;
        }

        let input_f32: Vec<f32> = values.iter().map(|&v| v as f32).collect();
        let mut output_f32 = vec![0.0_f32; values.len()];
        apply_transform_logicle(&input_f32, self.params, &mut output_f32);
        for (dst, src) in out.iter_mut().zip(output_f32.iter()) {
            *dst = *src as f64;
        }
    }

    fn transform_id(&self) -> &str {
        "logicle"
    }
}
