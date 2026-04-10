use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;

use bitvec::{order::Lsb0, vec::BitVec as InnerBitVec};

use crate::event::EventMatrix;
use crate::gate::GateRegistry;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParameterName(pub Arc<str>);

impl ParameterName {
    pub fn new(name: impl AsRef<str>) -> Self {
        Self(Arc::<str>::from(name.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ParameterName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for ParameterName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ParameterName {
    fn from(value: String) -> Self {
        Self(Arc::<str>::from(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GateId(pub Arc<str>);

impl GateId {
    pub fn new(id: impl AsRef<str>) -> Self {
        Self(Arc::<str>::from(id.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for GateId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for GateId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for GateId {
    fn from(value: String) -> Self {
        Self(Arc::<str>::from(value))
    }
}

pub type BitVec = InnerBitVec<u64, Lsb0>;

pub trait Transform: Send + Sync + Clone + Debug {
    fn apply(&self, value: f64) -> f64;
    fn invert(&self, scaled: f64) -> f64;

    fn apply_batch(&self, values: &[f64], out: &mut [f64]) {
        debug_assert_eq!(values.len(), out.len());
        values
            .iter()
            .zip(out.iter_mut())
            .for_each(|(&v, o)| *o = self.apply(v));
    }

    fn transform_id(&self) -> &str;
}

pub trait Gate: Send + Sync + Debug {
    fn dimensions(&self) -> &[ParameterName];
    fn contains(&self, coords: &[f64]) -> bool;
    fn gate_id(&self) -> &GateId;
    fn parent_id(&self) -> Option<&GateId>;
}

pub trait ApplyGate: Gate {
    fn classify(&self, matrix: &EventMatrix, gate_map: &GateRegistry) -> BitVec;
}
