use std::fmt::Write;

use flow_gate_core::{
    gate::{EllipsoidDimension, GateKind, PolygonDimension, PolygonGate, RectangleDimension},
    FlowGateError, Gate, TransformKind,
};

use crate::{namespace, parse_bound_dimension, BoundDimension, FlowGateDocument};

pub struct FlowGateSerializer;

impl FlowGateSerializer {
    pub fn to_string(doc: &FlowGateDocument) -> Result<String, FlowGateError> {
        serialize_document(doc)
    }
}

pub fn serialize_document(doc: &FlowGateDocument) -> Result<String, FlowGateError> {
    let mut out = String::new();
    writeln!(&mut out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).ok();
    writeln!(
        &mut out,
        r#"<gating:Gating-ML xmlns:gating="{}" xmlns:transforms="{}" xmlns:data-type="{}">"#,
        namespace::NS_GATING,
        namespace::NS_TRANSFORMS,
        namespace::NS_DATATYPE
    )
    .ok();

    let mut transform_ids: Vec<&str> = doc
        .transforms
        .keys()
        .map(String::as_str)
        .chain(doc.ratio_transforms.keys().map(String::as_str))
        .collect();
    transform_ids.sort_unstable();
    transform_ids.dedup();

    for id in transform_ids {
        if let Some(transform) = doc.transforms.get(id) {
            writeln!(
                &mut out,
                r#"  <transforms:transformation transforms:id="{}">"#,
                xml_escape(id)
            )
            .ok();
            write_transform_element(&mut out, transform)?;
            writeln!(&mut out, "  </transforms:transformation>").ok();
            continue;
        }
        if let Some(ratio) = doc.ratio_transforms.get(id) {
            writeln!(
                &mut out,
                r#"  <transforms:transformation transforms:id="{}">"#,
                xml_escape(id)
            )
            .ok();
            writeln!(
                &mut out,
                r#"    <transforms:fratio transforms:A="{:.15e}" transforms:B="{:.15e}" transforms:C="{:.15e}">"#,
                ratio.a, ratio.b, ratio.c
            )
            .ok();
            writeln!(
                &mut out,
                r#"      <data-type:fcs-dimension data-type:name="{}"/>"#,
                xml_escape(ratio.numerator.as_str())
            )
            .ok();
            writeln!(
                &mut out,
                r#"      <data-type:fcs-dimension data-type:name="{}"/>"#,
                xml_escape(ratio.denominator.as_str())
            )
            .ok();
            writeln!(&mut out, "    </transforms:fratio>").ok();
            writeln!(&mut out, "  </transforms:transformation>").ok();
        }
    }

    let mut spectrum_entries: Vec<_> = doc.spectrum_matrices.iter().collect();
    spectrum_entries.sort_by(|a, b| a.0.cmp(b.0));
    for (_id, spec) in spectrum_entries {
        if spec.n_rows() == 0 || spec.n_cols() == 0 {
            continue;
        }
        if spec.matrix_inverted_already {
            writeln!(
                &mut out,
                r#"  <transforms:spectrumMatrix transforms:id="{}" transforms:matrix-inverted-already="true">"#,
                xml_escape(&spec.id)
            )
            .ok();
        } else {
            writeln!(
                &mut out,
                r#"  <transforms:spectrumMatrix transforms:id="{}">"#,
                xml_escape(&spec.id)
            )
            .ok();
        }

        writeln!(&mut out, "    <transforms:fluorochromes>").ok();
        for dim in &spec.fluorochromes {
            writeln!(
                &mut out,
                r#"      <data-type:fcs-dimension data-type:name="{}"/>"#,
                xml_escape(dim.as_str())
            )
            .ok();
        }
        writeln!(&mut out, "    </transforms:fluorochromes>").ok();

        writeln!(&mut out, "    <transforms:detectors>").ok();
        for dim in &spec.detectors {
            writeln!(
                &mut out,
                r#"      <data-type:fcs-dimension data-type:name="{}"/>"#,
                xml_escape(dim.as_str())
            )
            .ok();
        }
        writeln!(&mut out, "    </transforms:detectors>").ok();

        let n_rows = spec.n_rows();
        let n_cols = spec.n_cols();
        for row in 0..n_rows {
            writeln!(&mut out, "    <transforms:spectrum>").ok();
            for col in 0..n_cols {
                let idx = row * n_cols + col;
                if let Some(value) = spec.coefficients.get(idx) {
                    writeln!(
                        &mut out,
                        r#"      <transforms:coefficient transforms:value="{:.15e}"/>"#,
                        value
                    )
                    .ok();
                }
            }
            writeln!(&mut out, "    </transforms:spectrum>").ok();
        }
        writeln!(&mut out, "  </transforms:spectrumMatrix>").ok();
    }

    for gate_id in doc.gate_registry.topological_order() {
        let gate = doc
            .gate_registry
            .get(gate_id)
            .ok_or_else(|| FlowGateError::UnknownGateReference(gate_id.clone(), gate_id.clone()))?;
        write_gate_element(&mut out, doc, gate)?;
    }

    writeln!(&mut out, "</gating:Gating-ML>").ok();
    Ok(out)
}

fn write_transform_element(
    out: &mut String,
    transform: &TransformKind,
) -> Result<(), FlowGateError> {
    match transform {
        TransformKind::Logicle(t) => {
            writeln!(
                out,
                r#"    <transforms:logicle transforms:T="{:.15e}" transforms:W="{:.15e}" transforms:M="{:.15e}" transforms:A="{:.15e}"/>"#,
                t.params.t, t.params.w, t.params.m, t.params.a
            )
            .ok();
        }
        TransformKind::FASinh(t) => {
            writeln!(
                out,
                r#"    <transforms:fasinh transforms:T="{:.15e}" transforms:M="{:.15e}" transforms:A="{:.15e}"/>"#,
                t.t, t.m, t.a
            )
            .ok();
        }
        TransformKind::Logarithmic(t) => {
            writeln!(
                out,
                r#"    <transforms:flog transforms:T="{:.15e}" transforms:M="{:.15e}"/>"#,
                t.t, t.m
            )
            .ok();
        }
        TransformKind::Linear(t) => {
            writeln!(
                out,
                r#"    <transforms:flin transforms:T="{:.15e}" transforms:A="{:.15e}"/>"#,
                t.t, t.a
            )
            .ok();
        }
        TransformKind::Hyperlog(t) => {
            writeln!(
                out,
                r#"    <transforms:hyperlog transforms:T="{:.15e}" transforms:W="{:.15e}" transforms:M="{:.15e}" transforms:A="{:.15e}"/>"#,
                t.t, t.w, t.m, t.a
            )
            .ok();
        }
    }
    Ok(())
}

fn write_gate_element(
    out: &mut String,
    doc: &FlowGateDocument,
    gate: &GateKind,
) -> Result<(), FlowGateError> {
    match gate {
        GateKind::Rectangle(g) => {
            write_gate_open(out, "RectangleGate", gate)?;
            for dim in g.rectangle_dimensions() {
                write_rectangle_dimension(out, doc, dim)?;
            }
            writeln!(out, "  </gating:RectangleGate>").ok();
        }
        GateKind::Polygon(g) => {
            write_gate_open(out, "PolygonGate", gate)?;
            write_polygon_gate(out, doc, g)?;
            writeln!(out, "  </gating:PolygonGate>").ok();
        }
        GateKind::Ellipsoid(g) => {
            write_gate_open(out, "EllipsoidGate", gate)?;
            for dim in g.dimensions_def() {
                write_unbounded_dimension(out, doc, dim, false)?;
            }
            writeln!(out, "    <gating:mean>").ok();
            for value in g.mean() {
                writeln!(
                    out,
                    r#"      <gating:coordinate data-type:value="{:.15e}"/>"#,
                    value
                )
                .ok();
            }
            writeln!(out, "    </gating:mean>").ok();

            writeln!(out, "    <gating:covarianceMatrix>").ok();
            let n = g.covariance().n();
            if g.covariance().uses_general_inverse() {
                let full = g.covariance().full_matrix();
                for row in 0..n {
                    writeln!(out, "      <gating:row>").ok();
                    for col in 0..n {
                        writeln!(
                            out,
                            r#"        <gating:entry data-type:value="{:.15e}"/>"#,
                            full[row * n + col]
                        )
                        .ok();
                    }
                    writeln!(out, "      </gating:row>").ok();
                }
            } else {
                let upper = g.covariance().to_upper_triangular();
                let mut idx = 0usize;
                for row in 0..n {
                    writeln!(out, "      <gating:row>").ok();
                    for _col in row..n {
                        writeln!(
                            out,
                            r#"        <gating:entry data-type:value="{:.15e}"/>"#,
                            upper[idx]
                        )
                        .ok();
                        idx += 1;
                    }
                    writeln!(out, "      </gating:row>").ok();
                }
            }
            writeln!(out, "    </gating:covarianceMatrix>").ok();
            writeln!(
                out,
                r#"    <gating:distanceSquare data-type:value="{:.15e}"/>"#,
                g.distance_sq()
            )
            .ok();
            writeln!(out, "  </gating:EllipsoidGate>").ok();
        }
        GateKind::Boolean(g) => {
            write_gate_open(out, "BooleanGate", gate)?;
            let tag = match g.op() {
                flow_gate_core::gate::BooleanOp::And => "and",
                flow_gate_core::gate::BooleanOp::Or => "or",
                flow_gate_core::gate::BooleanOp::Not => "not",
            };
            writeln!(out, "    <gating:{tag}>").ok();
            for op in g.operands() {
                if op.complement {
                    writeln!(
                        out,
                        r#"      <gating:gateReference gating:ref="{}" gating:use-as-complement="true"/>"#,
                        xml_escape(op.gate_id.as_str())
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        r#"      <gating:gateReference gating:ref="{}"/>"#,
                        xml_escape(op.gate_id.as_str())
                    )
                    .ok();
                }
            }
            writeln!(out, "    </gating:{tag}>").ok();
            writeln!(out, "  </gating:BooleanGate>").ok();
        }
    }
    Ok(())
}

fn write_gate_open(out: &mut String, tag: &str, gate: &GateKind) -> Result<(), FlowGateError> {
    let gate_id = xml_escape(gate.gate_id().as_str());
    if let Some(parent) = gate.parent_id() {
        writeln!(
            out,
            r#"  <gating:{tag} gating:id="{}" gating:parent_id="{}">"#,
            gate_id,
            xml_escape(parent.as_str())
        )
        .ok();
    } else {
        writeln!(out, r#"  <gating:{tag} gating:id="{}">"#, gate_id).ok();
    }
    Ok(())
}

fn write_rectangle_dimension(
    out: &mut String,
    doc: &FlowGateDocument,
    dim: &RectangleDimension,
) -> Result<(), FlowGateError> {
    let mut attrs = String::new();
    if let Some(min) = dim.min {
        write!(&mut attrs, r#" gating:min="{:.15e}""#, min).ok();
    }
    if let Some(max) = dim.max {
        write!(&mut attrs, r#" gating:max="{:.15e}""#, max).ok();
    }
    if let Some(tid) = dim.transform.and_then(|t| transform_id_for(doc, &t)) {
        write!(
            &mut attrs,
            r#" gating:transformation-ref="{}""#,
            xml_escape(tid)
        )
        .ok();
    }

    let (comp_ref, dim_xml) = dimension_reference_xml(&dim.parameter);
    write!(
        out,
        r#"    <gating:dimension gating:compensation-ref="{}"{}>"#,
        xml_escape(&comp_ref),
        attrs
    )
    .ok();
    writeln!(out).ok();
    writeln!(out, "      {dim_xml}").ok();
    writeln!(out, "    </gating:dimension>").ok();
    Ok(())
}

fn write_unbounded_dimension(
    out: &mut String,
    doc: &FlowGateDocument,
    dim: &impl DimensionLike,
    _indent_polygon: bool,
) -> Result<(), FlowGateError> {
    let mut attrs = String::new();
    if let Some(tid) = dim.transform().and_then(|t| transform_id_for(doc, &t)) {
        write!(
            &mut attrs,
            r#" gating:transformation-ref="{}""#,
            xml_escape(tid)
        )
        .ok();
    }

    let (comp_ref, dim_xml) = dimension_reference_xml(dim.parameter());
    let indent = "    ";
    writeln!(
        out,
        r#"{indent}<gating:dimension gating:compensation-ref="{}"{}>"#,
        xml_escape(&comp_ref),
        attrs
    )
    .ok();
    writeln!(out, "{indent}  {dim_xml}").ok();
    writeln!(out, "{indent}</gating:dimension>").ok();
    Ok(())
}

fn write_polygon_gate(
    out: &mut String,
    doc: &FlowGateDocument,
    g: &PolygonGate,
) -> Result<(), FlowGateError> {
    let dims: [&PolygonDimension; 2] = [g.x_dim(), g.y_dim()];
    for dim in dims {
        write_unbounded_dimension(out, doc, dim, true)?;
    }

    for (x, y) in g.vertices() {
        writeln!(out, "    <gating:vertex>").ok();
        writeln!(
            out,
            r#"      <gating:coordinate data-type:value="{:.15e}"/>"#,
            x
        )
        .ok();
        writeln!(
            out,
            r#"      <gating:coordinate data-type:value="{:.15e}"/>"#,
            y
        )
        .ok();
        writeln!(out, "    </gating:vertex>").ok();
    }
    Ok(())
}

trait DimensionLike {
    fn parameter(&self) -> &flow_gate_core::ParameterName;
    fn transform(&self) -> Option<TransformKind>;
}

impl DimensionLike for PolygonDimension {
    fn parameter(&self) -> &flow_gate_core::ParameterName {
        &self.parameter
    }

    fn transform(&self) -> Option<TransformKind> {
        self.transform
    }
}

impl DimensionLike for EllipsoidDimension {
    fn parameter(&self) -> &flow_gate_core::ParameterName {
        &self.parameter
    }

    fn transform(&self) -> Option<TransformKind> {
        self.transform
    }
}

fn dimension_reference_xml(parameter: &flow_gate_core::ParameterName) -> (String, String) {
    match parse_bound_dimension(parameter) {
        Some(BoundDimension::Fcs {
            compensation_ref,
            name,
        }) => (
            compensation_ref,
            format!(
                r#"<data-type:fcs-dimension data-type:name="{}"/>"#,
                xml_escape(&name)
            ),
        ),
        Some(BoundDimension::Ratio {
            compensation_ref,
            ratio_id,
        }) => (
            compensation_ref,
            format!(
                r#"<data-type:new-dimension data-type:transformation-ref="{}"/>"#,
                xml_escape(&ratio_id)
            ),
        ),
        None => (
            "uncompensated".to_string(),
            format!(
                r#"<data-type:parameter data-type:name="{}"/>"#,
                xml_escape(parameter.as_str())
            ),
        ),
    }
}

fn transform_id_for<'a>(doc: &'a FlowGateDocument, target: &TransformKind) -> Option<&'a str> {
    doc.transforms
        .iter()
        .find_map(|(id, t)| if t == target { Some(id.as_str()) } else { None })
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
