use crate::traits::Transform;

use super::{
    FASinhTransform, HyperlogTransform, LinearTransform, LogarithmicTransform, LogicleTransform,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformKind {
    Logicle(LogicleTransform),
    FASinh(FASinhTransform),
    Logarithmic(LogarithmicTransform),
    Linear(LinearTransform),
    Hyperlog(HyperlogTransform),
}

impl Transform for TransformKind {
    fn apply(&self, value: f64) -> f64 {
        match self {
            Self::Logicle(t) => t.apply(value),
            Self::FASinh(t) => t.apply(value),
            Self::Logarithmic(t) => t.apply(value),
            Self::Linear(t) => t.apply(value),
            Self::Hyperlog(t) => t.apply(value),
        }
    }

    fn invert(&self, scaled: f64) -> f64 {
        match self {
            Self::Logicle(t) => t.invert(scaled),
            Self::FASinh(t) => t.invert(scaled),
            Self::Logarithmic(t) => t.invert(scaled),
            Self::Linear(t) => t.invert(scaled),
            Self::Hyperlog(t) => t.invert(scaled),
        }
    }

    fn apply_batch(&self, values: &[f64], out: &mut [f64]) {
        match self {
            Self::Logicle(t) => t.apply_batch(values, out),
            Self::FASinh(t) => t.apply_batch(values, out),
            Self::Logarithmic(t) => t.apply_batch(values, out),
            Self::Linear(t) => t.apply_batch(values, out),
            Self::Hyperlog(t) => t.apply_batch(values, out),
        }
    }

    fn transform_id(&self) -> &str {
        match self {
            Self::Logicle(t) => t.transform_id(),
            Self::FASinh(t) => t.transform_id(),
            Self::Logarithmic(t) => t.transform_id(),
            Self::Linear(t) => t.transform_id(),
            Self::Hyperlog(t) => t.transform_id(),
        }
    }
}
