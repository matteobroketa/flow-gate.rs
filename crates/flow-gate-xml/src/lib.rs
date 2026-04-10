mod evaluator;
mod namespace;
mod parser;
pub mod schema;
mod serializer;

use std::collections::HashMap;

use flow_gate_core::{
    gate::GateKind, BitVec, EventMatrix, EventMatrixView, GateId, GateRegistry, FlowGateError,
    ParameterName, TransformKind,
};

pub use parser::FlowGateParser;
pub use serializer::FlowGateSerializer;

#[derive(Debug, Clone)]
pub struct RatioTransformSpec {
    pub id: String,
    pub numerator: ParameterName,
    pub denominator: ParameterName,
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

#[derive(Debug, Clone)]
pub struct SpectrumMatrixSpec {
    pub id: String,
    pub fluorochromes: Vec<ParameterName>,
    pub detectors: Vec<ParameterName>,
    pub coefficients: Vec<f64>, // row-major: fluorochrome rows x detector columns
    pub matrix_inverted_already: bool,
}

impl SpectrumMatrixSpec {
    pub fn n_rows(&self) -> usize {
        self.fluorochromes.len()
    }

    pub fn n_cols(&self) -> usize {
        self.detectors.len()
    }
}

pub struct FlowGateDocument {
    pub transforms: HashMap<String, TransformKind>,
    pub ratio_transforms: HashMap<String, RatioTransformSpec>,
    pub spectrum_matrices: HashMap<String, SpectrumMatrixSpec>,
    pub gate_registry: GateRegistry,
    pub source_xml: Option<String>,
}

impl FlowGateDocument {
    pub fn parse_str(xml: &str) -> Result<Self, FlowGateError> {
        parser::parse_document(xml)
    }

    pub fn classify(&self, matrix: &EventMatrix) -> Result<HashMap<GateId, BitVec>, FlowGateError> {
        self.classify_with_fcs_compensation(matrix, None)
    }

    pub fn classify_view(
        &self,
        matrix: &EventMatrixView<'_>,
    ) -> Result<HashMap<GateId, BitVec>, FlowGateError> {
        self.classify_view_with_fcs_compensation(matrix, None)
    }

    pub fn classify_with_fcs_compensation(
        &self,
        matrix: &EventMatrix,
        fcs_compensation: Option<&SpectrumMatrixSpec>,
    ) -> Result<HashMap<GateId, BitVec>, FlowGateError> {
        let prepared = self.prepare_owned_matrix_with_fcs_compensation(matrix, fcs_compensation)?;
        self.gate_registry.classify_all(&prepared)
    }

    pub fn classify_view_with_fcs_compensation(
        &self,
        matrix: &EventMatrixView<'_>,
        fcs_compensation: Option<&SpectrumMatrixSpec>,
    ) -> Result<HashMap<GateId, BitVec>, FlowGateError> {
        let prepared = evaluator::prepare_matrix_from_view(self, matrix, fcs_compensation)?;
        self.gate_registry.classify_all(&prepared)
    }

    /// Builds the exact preprocessed matrix (compensation + ratio dimensions) used by classify().
    pub fn prepare_owned_matrix_with_fcs_compensation(
        &self,
        matrix: &EventMatrix,
        fcs_compensation: Option<&SpectrumMatrixSpec>,
    ) -> Result<EventMatrix, FlowGateError> {
        evaluator::prepare_owned_matrix(self, matrix, fcs_compensation)
    }

    pub fn to_xml(&self) -> Result<String, FlowGateError> {
        serializer::serialize_document(self)
    }

    pub fn gates(&self) -> impl Iterator<Item = (&GateId, &GateKind)> {
        self.gate_registry.iter()
    }
}

const SYN_FCS_PREFIX: &str = "__gml_fcs::";
const SYN_RATIO_PREFIX: &str = "__gml_ratio::";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundDimension {
    Fcs {
        compensation_ref: String,
        name: String,
    },
    Ratio {
        compensation_ref: String,
        ratio_id: String,
    },
}

pub(crate) fn make_fcs_binding_name(compensation_ref: &str, name: &str) -> ParameterName {
    ParameterName::from(format!(
        "{SYN_FCS_PREFIX}{}::{}",
        escape_binding(compensation_ref),
        escape_binding(name),
    ))
}

pub(crate) fn make_ratio_binding_name(compensation_ref: &str, ratio_id: &str) -> ParameterName {
    ParameterName::from(format!(
        "{SYN_RATIO_PREFIX}{}::{}",
        escape_binding(compensation_ref),
        escape_binding(ratio_id),
    ))
}

pub(crate) fn parse_bound_dimension(name: &ParameterName) -> Option<BoundDimension> {
    let raw = name.as_str();
    if let Some(rest) = raw.strip_prefix(SYN_FCS_PREFIX) {
        let (comp, dim) = rest.split_once("::")?;
        return Some(BoundDimension::Fcs {
            compensation_ref: unescape_binding(comp),
            name: unescape_binding(dim),
        });
    }
    if let Some(rest) = raw.strip_prefix(SYN_RATIO_PREFIX) {
        let (comp, ratio) = rest.split_once("::")?;
        return Some(BoundDimension::Ratio {
            compensation_ref: unescape_binding(comp),
            ratio_id: unescape_binding(ratio),
        });
    }
    None
}

fn escape_binding(value: &str) -> String {
    value.replace('%', "%25").replace(':', "%3A")
}

fn unescape_binding(value: &str) -> String {
    value.replace("%3A", ":").replace("%25", "%")
}
