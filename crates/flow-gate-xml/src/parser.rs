use std::collections::HashMap;

use flow_gate_core::{
    gate::{
        BooleanGate, BooleanOp, BooleanOperand, EllipsoidDimension, EllipsoidGate, GateKind,
        PolygonDimension, PolygonGate, RectangleDimension, RectangleGate,
    },
    transform::{
        FASinhTransform, HyperlogTransform, LinearTransform, LogarithmicTransform, LogicleParams,
        LogicleTransform,
    },
    GateId, GateRegistry, FlowGateError, ParameterName, TransformKind,
};
use indexmap::IndexMap;
use quick_xml::{
    events::{BytesStart, Event},
    name::{Namespace, ResolveResult},
    NsReader,
};

use crate::{
    make_fcs_binding_name, make_ratio_binding_name,
    namespace::{parse_bool_attr, NS_DATATYPE, NS_GATING, NS_TRANSFORMS},
    FlowGateDocument, RatioTransformSpec, SpectrumMatrixSpec,
};

#[derive(Default)]
pub struct FlowGateParser {
    transforms: HashMap<String, TransformKind>,
    ratio_transforms: HashMap<String, RatioTransformSpec>,
    spectrum_matrices: HashMap<String, SpectrumMatrixSpec>,
    gates: IndexMap<GateId, GateKind>,
}

impl FlowGateParser {
    pub fn parse_str(xml: &str) -> Result<FlowGateDocument, FlowGateError> {
        parse_document(xml)
    }
}

#[derive(Debug, Clone)]
enum DimensionSelector {
    Fcs(String),
    New(String),
}

fn parse_boolean_op_block(
    reader: &mut NsReader<&[u8]>,
    source: &str,
    operator_local: &[u8],
    current_gate_id: &GateId,
    existing_gates: &IndexMap<GateId, GateKind>,
) -> Result<(BooleanOp, Vec<BooleanOperand>), FlowGateError> {
    let op = match operator_local {
        b"and" => BooleanOp::And,
        b"or" => BooleanOp::Or,
        b"not" => BooleanOp::Not,
        _ => {
            return Err(FlowGateError::InvalidGate(format!(
                "Unsupported Boolean operator '{}'",
                String::from_utf8_lossy(operator_local)
            )))
        }
    };

    let mut operands = Vec::new();
    let mut buf = Vec::new();
    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"gateReference") {
                    let attrs = attrs_map(reader, source, &e)?;
                    let gate_id = GateId::from(
                        required_attr(&attrs, "ref", &[NS_GATING, ""], "gateReference")?
                            .to_string(),
                    );
                    if !existing_gates.contains_key(&gate_id) {
                        return Err(FlowGateError::UnknownGateReference(
                            current_gate_id.clone(),
                            gate_id,
                        ));
                    }
                    let complement = parse_bool_attr(
                        optional_attr(&attrs, "use-as-complement", &[NS_GATING, ""])
                            .or_else(|| optional_attr(&attrs, "complement", &[NS_GATING, ""])),
                        false,
                    );
                    operands.push(BooleanOperand {
                        gate_id,
                        complement,
                    });
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"gateReference") {
                    let attrs = attrs_map(reader, source, &e)?;
                    let gate_id = GateId::from(
                        required_attr(&attrs, "ref", &[NS_GATING, ""], "gateReference")?
                            .to_string(),
                    );
                    if !existing_gates.contains_key(&gate_id) {
                        return Err(FlowGateError::UnknownGateReference(
                            current_gate_id.clone(),
                            gate_id,
                        ));
                    }
                    let complement = parse_bool_attr(
                        optional_attr(&attrs, "use-as-complement", &[NS_GATING, ""])
                            .or_else(|| optional_attr(&attrs, "complement", &[NS_GATING, ""])),
                        false,
                    );
                    operands.push(BooleanOperand {
                        gate_id,
                        complement,
                    });
                }
            }
            Event::End(e)
                if ns_is(&ns, NS_GATING) && e.name().local_name().as_ref() == operator_local =>
            {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading Boolean operator block",
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok((op, operands))
}

fn parse_vertex(reader: &mut NsReader<&[u8]>, source: &str) -> Result<(f64, f64), FlowGateError> {
    let coords = parse_coordinates_block(reader, source, NS_GATING, b"vertex")?;
    if coords.len() != 2 {
        return Err(FlowGateError::InvalidGate(format!(
            "Polygon vertex must contain exactly 2 coordinates, found {}",
            coords.len()
        )));
    }
    Ok((coords[0], coords[1]))
}

fn parse_coordinates_block(
    reader: &mut NsReader<&[u8]>,
    source: &str,
    end_ns: &str,
    end_local: &[u8],
) -> Result<Vec<f64>, FlowGateError> {
    let mut values = Vec::new();
    let mut buf = Vec::new();

    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"coordinate") {
                    let attrs = attrs_map(reader, source, &e)?;
                    values.push(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_DATATYPE, ""],
                        "coordinate",
                    )?);
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"coordinate") {
                    let attrs = attrs_map(reader, source, &e)?;
                    values.push(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_DATATYPE, ""],
                        "coordinate",
                    )?);
                }
            }
            Event::Text(t) => {
                let text = t
                    .unescape()
                    .map_err(|e| xml_err(reader, source, format!("XML text decode error: {e}")))?;
                for token in text.split_whitespace() {
                    if let Ok(v) = token.parse::<f64>() {
                        values.push(v);
                    }
                }
            }
            Event::End(e) if element_is(&ns, e.name().local_name().as_ref(), end_ns, end_local) => {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading coordinate block",
                ));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(values)
}

fn parse_covariance_block(
    reader: &mut NsReader<&[u8]>,
    source: &str,
) -> Result<Vec<f64>, FlowGateError> {
    let mut values = Vec::new();
    let mut buf = Vec::new();
    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"row") {
                    values.extend(parse_covariance_row(reader, source)?);
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::End(e)
                if element_is(
                    &ns,
                    e.name().local_name().as_ref(),
                    NS_GATING,
                    b"covarianceMatrix",
                ) =>
            {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading covarianceMatrix",
                ));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(values)
}

fn parse_covariance_row(
    reader: &mut NsReader<&[u8]>,
    source: &str,
) -> Result<Vec<f64>, FlowGateError> {
    let mut values = Vec::new();
    let mut buf = Vec::new();
    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"entry")
                    || element_is(&ns, e.local_name().as_ref(), NS_GATING, b"coordinate")
                {
                    let attrs = attrs_map(reader, source, &e)?;
                    values.push(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_DATATYPE, "", NS_GATING],
                        "entry",
                    )?);
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"entry")
                    || element_is(&ns, e.local_name().as_ref(), NS_GATING, b"coordinate")
                {
                    let attrs = attrs_map(reader, source, &e)?;
                    values.push(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_DATATYPE, "", NS_GATING],
                        "entry",
                    )?);
                }
            }
            Event::End(e) if element_is(&ns, e.name().local_name().as_ref(), NS_GATING, b"row") => {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading covariance row",
                ));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(values)
}

fn parse_transform_kind(
    reader: &NsReader<&[u8]>,
    source: &str,
    local: &[u8],
    start: &BytesStart<'_>,
) -> Result<TransformKind, FlowGateError> {
    let attrs = attrs_map(reader, source, start)?;
    match local {
        b"logicle" => {
            let t = parse_required_f64_attr(&attrs, "T", &[NS_TRANSFORMS, ""], "logicle")?;
            let w = parse_required_f64_attr(&attrs, "W", &[NS_TRANSFORMS, ""], "logicle")?;
            let m = parse_required_f64_attr(&attrs, "M", &[NS_TRANSFORMS, ""], "logicle")?;
            let a = parse_required_f64_attr(&attrs, "A", &[NS_TRANSFORMS, ""], "logicle")?;
            let params = LogicleParams { t, w, m, a }
                .validate()
                .map_err(FlowGateError::InvalidTransformParam)?;
            Ok(TransformKind::Logicle(LogicleTransform { params }))
        }
        b"fasinh" => {
            let t = parse_required_f64_attr(&attrs, "T", &[NS_TRANSFORMS, ""], "fasinh")?;
            let m = parse_required_f64_attr(&attrs, "M", &[NS_TRANSFORMS, ""], "fasinh")?;
            let a = parse_required_f64_attr(&attrs, "A", &[NS_TRANSFORMS, ""], "fasinh")?;
            Ok(TransformKind::FASinh(FASinhTransform::new(t, m, a)?))
        }
        b"log" | b"flog" => {
            let t = parse_required_f64_attr(&attrs, "T", &[NS_TRANSFORMS, ""], "log")?;
            let m = parse_required_f64_attr(&attrs, "M", &[NS_TRANSFORMS, ""], "log")?;
            Ok(TransformKind::Logarithmic(LogarithmicTransform::new(t, m)?))
        }
        b"lin" | b"flin" => {
            let t = parse_required_f64_attr(&attrs, "T", &[NS_TRANSFORMS, ""], "lin")?;
            let a = parse_required_f64_attr(&attrs, "A", &[NS_TRANSFORMS, ""], "lin")?;
            Ok(TransformKind::Linear(LinearTransform::new(t, a)?))
        }
        b"hyperlog" => {
            let t = parse_required_f64_attr(&attrs, "T", &[NS_TRANSFORMS, ""], "hyperlog")?;
            let w = parse_required_f64_attr(&attrs, "W", &[NS_TRANSFORMS, ""], "hyperlog")?;
            let m = parse_required_f64_attr(&attrs, "M", &[NS_TRANSFORMS, ""], "hyperlog")?;
            let a = parse_required_f64_attr(&attrs, "A", &[NS_TRANSFORMS, ""], "hyperlog")?;
            Ok(TransformKind::Hyperlog(HyperlogTransform::new(t, w, m, a)?))
        }
        _ => Err(FlowGateError::InvalidGate(format!(
            "Unsupported transform element '{}'",
            String::from_utf8_lossy(local)
        ))),
    }
}

fn parse_transform_ref(
    attrs: &[AttrRecord],
    transform_map: &HashMap<String, TransformKind>,
) -> Result<Option<TransformKind>, FlowGateError> {
    let candidates = [
        "transformation-ref",
        "transformation_ref",
        "transformationRef",
        "transformation",
        "transformRef",
        "transform_ref",
    ];
    for name in candidates {
        if let Some(value) = optional_attr(attrs, name, &[NS_GATING, ""]) {
            let transform = transform_map.get(value).copied().ok_or_else(|| {
                FlowGateError::InvalidGate(format!("Unknown transform reference '{value}'"))
            })?;
            return Ok(Some(transform));
        }
    }
    Ok(None)
}

fn attrs_map(
    reader: &NsReader<&[u8]>,
    source: &str,
    start: &BytesStart<'_>,
) -> Result<Vec<AttrRecord>, FlowGateError> {
    let mut out = Vec::new();
    for attr in start.attributes().with_checks(false) {
        let attr =
            attr.map_err(|e| xml_err(reader, source, format!("Invalid XML attribute: {e}")))?;
        let (resolved_ns, local_name) = reader.resolve_attribute(attr.key);
        let ns_uri = match resolved_ns {
            ResolveResult::Bound(Namespace(ns)) => Some(String::from_utf8_lossy(ns).to_string()),
            ResolveResult::Unbound => None,
            ResolveResult::Unknown(prefix) => {
                return Err(FlowGateError::XmlParse(format!(
                    "Unknown XML namespace prefix '{}' in attribute name",
                    String::from_utf8_lossy(&prefix)
                )))
            }
        };
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|e| xml_err(reader, source, format!("XML attribute decode error: {e}")))?
            .into_owned();
        out.push(AttrRecord {
            ns_uri,
            local: String::from_utf8_lossy(local_name.as_ref()).to_string(),
            value,
        });
    }
    Ok(out)
}

fn required_attr<'a>(
    attrs: &'a [AttrRecord],
    local: &str,
    allowed_ns: &[&str],
    element: &str,
) -> Result<&'a str, FlowGateError> {
    optional_attr(attrs, local, allowed_ns)
        .ok_or_else(|| FlowGateError::MissingAttribute(local.to_string(), element.to_string()))
}

fn optional_attr<'a>(attrs: &'a [AttrRecord], local: &str, allowed_ns: &[&str]) -> Option<&'a str> {
    for ns in allowed_ns {
        let ns_opt = if ns.is_empty() { None } else { Some(*ns) };
        if let Some(hit) = attrs
            .iter()
            .find(|a| a.local == local && a.ns_uri.as_deref() == ns_opt)
        {
            return Some(hit.value.as_str());
        }
    }
    None
}

fn parse_required_f64_attr(
    attrs: &[AttrRecord],
    local: &str,
    allowed_ns: &[&str],
    element: &str,
) -> Result<f64, FlowGateError> {
    let raw = required_attr(attrs, local, allowed_ns, element)?;
    raw.parse::<f64>()
        .map_err(|_| FlowGateError::InvalidFloat(raw.to_string(), local.to_string()))
}

fn parse_optional_f64_attr(
    attrs: &[AttrRecord],
    local: &str,
    allowed_ns: &[&str],
    element: &str,
) -> Result<Option<f64>, FlowGateError> {
    match optional_attr(attrs, local, allowed_ns) {
        Some(raw) if raw.trim().is_empty() => Ok(None),
        Some(raw) => raw
            .parse::<f64>()
            .map(Some)
            .map_err(|_| FlowGateError::InvalidFloat(raw.to_string(), format!("{element}:{local}"))),
        None => Ok(None),
    }
}

fn skip_element(
    reader: &mut NsReader<&[u8]>,
    source: &str,
    start: &BytesStart<'_>,
) -> Result<(), FlowGateError> {
    let mut buf = Vec::new();
    reader
        .read_to_end_into(start.name(), &mut buf)
        .map_err(|e| xml_err(reader, source, format!("XML skip error: {e}")))?;
    Ok(())
}

fn element_is(
    resolved_ns: &ResolveResult<'_>,
    local: &[u8],
    expected_ns: &str,
    expected_local: &[u8],
) -> bool {
    ns_is(resolved_ns, expected_ns) && local == expected_local
}

fn ns_is(resolved_ns: &ResolveResult<'_>, expected_ns: &str) -> bool {
    matches!(resolved_ns, ResolveResult::Bound(Namespace(ns)) if *ns == expected_ns.as_bytes())
}

fn xml_err(reader: &NsReader<&[u8]>, source: &str, message: impl AsRef<str>) -> FlowGateError {
    let pos = reader.error_position() as usize;
    let (line, col) = byte_pos_to_line_col(source, pos);
    FlowGateError::XmlParse(format!(
        "{} (line {}, column {})",
        message.as_ref(),
        line,
        col
    ))
}

fn byte_pos_to_line_col(source: &str, pos: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for &b in source.as_bytes().iter().take(pos.min(source.len())) {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[derive(Debug, Clone)]
struct AttrRecord {
    ns_uri: Option<String>,
    local: String,
    value: String,
}

fn is_gate_element(local: &[u8]) -> bool {
    matches!(
        local,
        b"RectangleGate" | b"PolygonGate" | b"EllipsoidGate" | b"BooleanGate" | b"QuadrantGate"
    )
}

fn parse_dimension_selector(
    reader: &NsReader<&[u8]>,
    source: &str,
    start: &BytesStart<'_>,
) -> Result<DimensionSelector, FlowGateError> {
    let local = start.local_name();
    let local = local.as_ref();
    if !matches!(local, b"parameter" | b"fcs-dimension" | b"new-dimension") {
        return Err(xml_err(
            reader,
            source,
            format!(
                "Unsupported dimension element '{}'",
                String::from_utf8_lossy(local)
            ),
        ));
    }

    let attrs = attrs_map(reader, source, start)?;
    if local == b"new-dimension" {
        let ratio_id = required_attr(
            &attrs,
            "transformation-ref",
            &[NS_DATATYPE, ""],
            "new-dimension",
        )?;
        Ok(DimensionSelector::New(ratio_id.to_string()))
    } else {
        let name = required_attr(&attrs, "name", &[NS_DATATYPE, ""], "fcs-dimension")?;
        Ok(DimensionSelector::Fcs(name.to_string()))
    }
}

fn dimension_selector_to_parameter(
    compensation_ref: &str,
    selector: DimensionSelector,
) -> ParameterName {
    match selector {
        DimensionSelector::Fcs(name) => make_fcs_binding_name(compensation_ref, &name),
        DimensionSelector::New(ratio_id) => make_ratio_binding_name(compensation_ref, &ratio_id),
    }
}

fn parse_dimension_ref(
    reader: &mut NsReader<&[u8]>,
    source: &str,
    attrs: &[AttrRecord],
    context: &str,
    end_ns: &str,
    end_local: &[u8],
) -> Result<ParameterName, FlowGateError> {
    let compensation_ref = optional_attr(attrs, "compensation-ref", &[NS_GATING, ""])
        .unwrap_or("uncompensated")
        .to_string();

    let mut selector: Option<DimensionSelector> = None;
    let mut buf = Vec::new();

    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if ns_is(&ns, NS_DATATYPE)
                    && matches!(
                        e.local_name().as_ref(),
                        b"parameter" | b"fcs-dimension" | b"new-dimension"
                    )
                {
                    if selector.is_some() {
                        return Err(FlowGateError::InvalidGate(format!(
                            "{context} contains multiple dimension selectors"
                        )));
                    }
                    selector = Some(parse_dimension_selector(reader, source, &e)?);
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if ns_is(&ns, NS_DATATYPE)
                    && matches!(
                        e.local_name().as_ref(),
                        b"parameter" | b"fcs-dimension" | b"new-dimension"
                    )
                {
                    if selector.is_some() {
                        return Err(FlowGateError::InvalidGate(format!(
                            "{context} contains multiple dimension selectors"
                        )));
                    }
                    selector = Some(parse_dimension_selector(reader, source, &e)?);
                }
            }
            Event::End(e) if element_is(&ns, e.name().local_name().as_ref(), end_ns, end_local) => {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    format!("Unexpected EOF while reading {context}"),
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    let selector = selector.ok_or_else(|| {
        FlowGateError::InvalidGate(format!("{context} is missing fcs-dimension/new-dimension"))
    })?;
    Ok(dimension_selector_to_parameter(&compensation_ref, selector))
}

fn parse_value_content(reader: &mut NsReader<&[u8]>, source: &str) -> Result<f64, FlowGateError> {
    let mut value: Option<f64> = None;
    let mut buf = Vec::new();

    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                let attrs = attrs_map(reader, source, &e)?;
                if let Some(v) = parse_optional_f64_attr(
                    &attrs,
                    "value",
                    &[NS_DATATYPE, "", NS_GATING, NS_TRANSFORMS],
                    "value",
                )? {
                    value = Some(v);
                }
                skip_element(reader, source, &e)?;
            }
            Event::Empty(e) => {
                let attrs = attrs_map(reader, source, &e)?;
                if let Some(v) = parse_optional_f64_attr(
                    &attrs,
                    "value",
                    &[NS_DATATYPE, "", NS_GATING, NS_TRANSFORMS],
                    "value",
                )? {
                    value = Some(v);
                }
            }
            Event::Text(t) => {
                let text = t
                    .unescape()
                    .map_err(|e| xml_err(reader, source, format!("XML text decode error: {e}")))?;
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let parsed = trimmed.parse::<f64>().map_err(|_| {
                        FlowGateError::InvalidFloat(trimmed.to_string(), "value".to_string())
                    })?;
                    value = Some(parsed);
                }
            }
            Event::End(e)
                if element_is(&ns, e.name().local_name().as_ref(), NS_GATING, b"value") =>
            {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading <value>",
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    value.ok_or_else(|| FlowGateError::InvalidGate("Missing numeric value in <value>".to_string()))
}

fn parse_fcs_dimension_list(
    reader: &mut NsReader<&[u8]>,
    source: &str,
    end_ns: &str,
    end_local: &[u8],
) -> Result<Vec<ParameterName>, FlowGateError> {
    let mut out = Vec::new();
    let mut buf = Vec::new();
    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if ns_is(&ns, NS_DATATYPE)
                    && matches!(e.local_name().as_ref(), b"fcs-dimension" | b"parameter")
                {
                    let attrs = attrs_map(reader, source, &e)?;
                    let name = required_attr(&attrs, "name", &[NS_DATATYPE, ""], "fcs-dimension")?;
                    out.push(ParameterName::from(name.to_string()));
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if ns_is(&ns, NS_DATATYPE)
                    && matches!(e.local_name().as_ref(), b"fcs-dimension" | b"parameter")
                {
                    let attrs = attrs_map(reader, source, &e)?;
                    let name = required_attr(&attrs, "name", &[NS_DATATYPE, ""], "fcs-dimension")?;
                    out.push(ParameterName::from(name.to_string()));
                }
            }
            Event::End(e) if element_is(&ns, e.name().local_name().as_ref(), end_ns, end_local) => {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading fcs-dimension list",
                ));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

fn parse_spectrum_row(reader: &mut NsReader<&[u8]>, source: &str) -> Result<Vec<f64>, FlowGateError> {
    let mut values = Vec::new();
    let mut buf = Vec::new();
    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_TRANSFORMS, b"coefficient") {
                    let attrs = attrs_map(reader, source, &e)?;
                    values.push(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_TRANSFORMS, "", NS_DATATYPE],
                        "coefficient",
                    )?);
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_TRANSFORMS, b"coefficient") {
                    let attrs = attrs_map(reader, source, &e)?;
                    values.push(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_TRANSFORMS, "", NS_DATATYPE],
                        "coefficient",
                    )?);
                }
            }
            Event::End(e)
                if element_is(
                    &ns,
                    e.name().local_name().as_ref(),
                    NS_TRANSFORMS,
                    b"spectrum",
                ) =>
            {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading <spectrum>",
                ));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(values)
}

fn parse_quadrant_definition(
    reader: &mut NsReader<&[u8]>,
    source: &str,
    start: &BytesStart<'_>,
) -> Result<(GateId, Vec<(String, f64)>), FlowGateError> {
    let attrs = attrs_map(reader, source, start)?;
    let id = GateId::from(required_attr(&attrs, "id", &[NS_GATING, ""], "Quadrant")?.to_string());
    let mut positions: Vec<(String, f64)> = Vec::new();
    let mut buf = Vec::new();
    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"position") {
                    let pos_attrs = attrs_map(reader, source, &e)?;
                    let divider_ref =
                        required_attr(&pos_attrs, "divider_ref", &[NS_GATING, ""], "position")?
                            .to_string();
                    let location = parse_required_f64_attr(
                        &pos_attrs,
                        "location",
                        &[NS_GATING, ""],
                        "position",
                    )?;
                    positions.push((divider_ref, location));
                    skip_element(reader, source, &e)?;
                } else {
                    skip_element(reader, source, &e)?;
                }
            }
            Event::Empty(e) => {
                if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"position") {
                    let pos_attrs = attrs_map(reader, source, &e)?;
                    let divider_ref =
                        required_attr(&pos_attrs, "divider_ref", &[NS_GATING, ""], "position")?
                            .to_string();
                    let location = parse_required_f64_attr(
                        &pos_attrs,
                        "location",
                        &[NS_GATING, ""],
                        "position",
                    )?;
                    positions.push((divider_ref, location));
                }
            }
            Event::End(e)
                if element_is(&ns, e.name().local_name().as_ref(), NS_GATING, b"Quadrant") =>
            {
                break;
            }
            Event::Eof => {
                return Err(xml_err(
                    reader,
                    source,
                    "Unexpected EOF while reading <Quadrant>",
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    if positions.is_empty() {
        return Err(FlowGateError::InvalidGate(format!(
            "Quadrant '{}' does not define any positions",
            id
        )));
    }

    Ok((id, positions))
}

fn interval_for_location(
    values: &[f64],
    location: f64,
) -> Result<(Option<f64>, Option<f64>), FlowGateError> {
    if !location.is_finite() {
        return Err(FlowGateError::InvalidGate(
            "Quadrant position location must be finite".to_string(),
        ));
    }
    if values.is_empty() {
        return Err(FlowGateError::InvalidGate(
            "Quadrant divider must define at least one value".to_string(),
        ));
    }

    let mut sorted: Vec<f64> = values.to_vec();
    if sorted.iter().any(|v| !v.is_finite()) {
        return Err(FlowGateError::InvalidGate(
            "Quadrant divider values must be finite".to_string(),
        ));
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    sorted.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);

    if location < sorted[0] {
        return Ok((None, Some(sorted[0])));
    }
    for pair in sorted.windows(2) {
        let lo = pair[0];
        let hi = pair[1];
        if location >= lo && location < hi {
            return Ok((Some(lo), Some(hi)));
        }
    }
    Ok((Some(*sorted.last().expect("non-empty")), None))
}

pub fn parse_document(xml: &str) -> Result<FlowGateDocument, FlowGateError> {
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().trim_text(true);
    reader.config_mut().expand_empty_elements = true;

    let mut parser = FlowGateParser::default();
    let mut buf = Vec::new();

    loop {
        let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok(v) => v,
            Err(e) => return Err(xml_err(&reader, xml, format!("XML read error: {e}"))),
        };
        match event {
            Event::Start(e) => {
                let local_name = e.local_name();
                let local = local_name.as_ref();
                if element_is(&ns, local, NS_TRANSFORMS, b"transformation") {
                    parser.parse_transformation_block(&mut reader, xml, &e)?;
                } else if element_is(&ns, local, NS_TRANSFORMS, b"spectrumMatrix") {
                    parser.parse_spectrum_matrix_block(&mut reader, xml, &e)?;
                } else if element_is(&ns, local, NS_GATING, b"Gate") {
                    parser.parse_gate_block(&mut reader, xml, &e)?;
                } else if ns_is(&ns, NS_GATING) && is_gate_element(local) {
                    parser.parse_standalone_gate(&mut reader, xml, &e)?;
                } else if element_is(&ns, local, NS_GATING, b"GatingML")
                    || element_is(&ns, local, NS_GATING, b"Gating-ML")
                {
                    // Root container; continue scanning nested elements.
                } else {
                    skip_element(&mut reader, xml, &e)?;
                }
            }
            Event::Empty(_) => {}
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    let gate_registry = GateRegistry::new(parser.gates)?;
    Ok(FlowGateDocument {
        transforms: parser.transforms,
        ratio_transforms: parser.ratio_transforms,
        spectrum_matrices: parser.spectrum_matrices,
        gate_registry,
        source_xml: Some(xml.to_string()),
    })
}

impl FlowGateParser {
    fn parse_transformation_block(
        &mut self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<(), FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let id = required_attr(&attrs, "id", &[NS_TRANSFORMS, ""], "transformation")?.to_string();
        let mut selected_scale: Option<TransformKind> = None;
        let mut selected_ratio: Option<RatioTransformSpec> = None;

        let mut buf = Vec::new();
        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) if ns_is(&ns, NS_TRANSFORMS) => {
                    if e.local_name().as_ref() == b"fratio" {
                        if selected_scale.is_some() || selected_ratio.is_some() {
                            return Err(FlowGateError::InvalidGate(format!(
                                "Transformation '{id}' contains multiple payload elements"
                            )));
                        }
                        selected_ratio = Some(self.parse_ratio_transform(reader, source, &e, &id)?);
                    } else {
                        if selected_scale.is_some() || selected_ratio.is_some() {
                            return Err(FlowGateError::InvalidGate(format!(
                                "Transformation '{id}' contains multiple payload elements"
                            )));
                        }
                        selected_scale = Some(parse_transform_kind(
                            reader,
                            source,
                            e.local_name().as_ref(),
                            &e,
                        )?);
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::Empty(e) => {
                    if ns_is(&ns, NS_TRANSFORMS) {
                        if e.local_name().as_ref() == b"fratio" {
                            return Err(FlowGateError::InvalidGate(
                                "fratio requires two fcs-dimension sub-elements".to_string(),
                            ));
                        }
                        if selected_scale.is_some() || selected_ratio.is_some() {
                            return Err(FlowGateError::InvalidGate(format!(
                                "Transformation '{id}' contains multiple payload elements"
                            )));
                        }
                        selected_scale = Some(parse_transform_kind(
                            reader,
                            source,
                            e.local_name().as_ref(),
                            &e,
                        )?);
                    }
                }
                Event::End(e)
                    if ns_is(&ns, NS_TRANSFORMS)
                        && e.name().local_name().as_ref() == b"transformation" =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading <transformation>",
                    ));
                }
                Event::Start(e) => {
                    skip_element(reader, source, &e)?;
                }
                _ => {}
            }
            buf.clear();
        }

        match (selected_scale, selected_ratio) {
            (Some(scale), None) => {
                self.transforms.insert(id, scale);
                Ok(())
            }
            (None, Some(ratio)) => {
                self.ratio_transforms.insert(id, ratio);
                Ok(())
            }
            (None, None) => Err(xml_err(
                reader,
                source,
                "Transformation block does not contain a supported transform element",
            )),
            (Some(_), Some(_)) => Err(FlowGateError::InvalidGate(
                "Transformation cannot contain both scale and ratio payloads".to_string(),
            )),
        }
    }

    fn parse_ratio_transform(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
        id: &str,
    ) -> Result<RatioTransformSpec, FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let a = parse_required_f64_attr(&attrs, "A", &[NS_TRANSFORMS, ""], "fratio")?;
        let b = parse_required_f64_attr(&attrs, "B", &[NS_TRANSFORMS, ""], "fratio")?;
        let c = parse_required_f64_attr(&attrs, "C", &[NS_TRANSFORMS, ""], "fratio")?;
        let mut dims: Vec<ParameterName> = Vec::new();

        let mut buf = Vec::new();
        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if ns_is(&ns, NS_DATATYPE)
                        && matches!(e.local_name().as_ref(), b"fcs-dimension" | b"parameter")
                    {
                        let param_attrs = attrs_map(reader, source, &e)?;
                        let name = required_attr(
                            &param_attrs,
                            "name",
                            &[NS_DATATYPE, ""],
                            "fcs-dimension",
                        )?;
                        dims.push(ParameterName::from(name.to_string()));
                        skip_element(reader, source, &e)?;
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::Empty(e) => {
                    if ns_is(&ns, NS_DATATYPE)
                        && matches!(e.local_name().as_ref(), b"fcs-dimension" | b"parameter")
                    {
                        let param_attrs = attrs_map(reader, source, &e)?;
                        let name = required_attr(
                            &param_attrs,
                            "name",
                            &[NS_DATATYPE, ""],
                            "fcs-dimension",
                        )?;
                        dims.push(ParameterName::from(name.to_string()));
                    }
                }
                Event::End(e)
                    if ns_is(&ns, NS_TRANSFORMS) && e.name().local_name().as_ref() == b"fratio" =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading fratio transformation",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        if dims.len() != 2 {
            return Err(FlowGateError::InvalidGate(format!(
                "fratio transformation '{id}' requires exactly 2 dimensions, found {}",
                dims.len()
            )));
        }

        Ok(RatioTransformSpec {
            id: id.to_string(),
            numerator: dims.remove(0),
            denominator: dims.remove(0),
            a,
            b,
            c,
        })
    }

    fn parse_spectrum_matrix_block(
        &mut self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<(), FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let id = required_attr(&attrs, "id", &[NS_TRANSFORMS, ""], "spectrumMatrix")?.to_string();
        let matrix_inverted_already = parse_bool_attr(
            optional_attr(&attrs, "matrix-inverted-already", &[NS_TRANSFORMS, ""]),
            false,
        );
        let mut fluorochromes: Vec<ParameterName> = Vec::new();
        let mut detectors: Vec<ParameterName> = Vec::new();
        let mut rows: Vec<Vec<f64>> = Vec::new();

        let mut buf = Vec::new();
        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if element_is(
                        &ns,
                        e.local_name().as_ref(),
                        NS_TRANSFORMS,
                        b"fluorochromes",
                    ) {
                        fluorochromes = parse_fcs_dimension_list(
                            reader,
                            source,
                            NS_TRANSFORMS,
                            b"fluorochromes",
                        )?;
                    } else if element_is(&ns, e.local_name().as_ref(), NS_TRANSFORMS, b"detectors")
                    {
                        detectors =
                            parse_fcs_dimension_list(reader, source, NS_TRANSFORMS, b"detectors")?;
                    } else if element_is(&ns, e.local_name().as_ref(), NS_TRANSFORMS, b"spectrum") {
                        rows.push(parse_spectrum_row(reader, source)?);
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::End(e)
                    if element_is(
                        &ns,
                        e.name().local_name().as_ref(),
                        NS_TRANSFORMS,
                        b"spectrumMatrix",
                    ) =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading spectrumMatrix",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        if fluorochromes.is_empty() || detectors.is_empty() {
            return Err(FlowGateError::InvalidGate(format!(
                "spectrumMatrix '{id}' must define fluorochromes and detectors"
            )));
        }
        if rows.len() != fluorochromes.len() {
            return Err(FlowGateError::InvalidGate(format!(
                "spectrumMatrix '{id}' has {} rows but {} fluorochromes",
                rows.len(),
                fluorochromes.len()
            )));
        }
        for row in &rows {
            if row.len() != detectors.len() {
                return Err(FlowGateError::InvalidGate(format!(
                    "spectrumMatrix '{id}' row has {} coefficients but {} detectors",
                    row.len(),
                    detectors.len()
                )));
            }
        }

        let mut coefficients = Vec::with_capacity(fluorochromes.len() * detectors.len());
        for row in rows {
            coefficients.extend(row);
        }

        self.spectrum_matrices.insert(
            id.clone(),
            SpectrumMatrixSpec {
                id,
                fluorochromes,
                detectors,
                coefficients,
                matrix_inverted_already,
            },
        );
        Ok(())
    }

    fn parse_gate_block(
        &mut self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<(), FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let id = GateId::from(required_attr(&attrs, "id", &[NS_GATING, ""], "Gate")?.to_string());
        let parent_id = optional_attr(&attrs, "parent_id", &[NS_GATING, ""])
            .map(|s| GateId::from(s.to_string()));
        let mut parsed_gate: Option<GateKind> = None;

        let mut buf = Vec::new();
        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if ns_is(&ns, NS_GATING) {
                        parsed_gate = Some(match e.local_name().as_ref() {
                            b"RectangleGate" => GateKind::Rectangle(self.parse_rectangle_gate(
                                reader,
                                source,
                                id.clone(),
                                parent_id.clone(),
                            )?),
                            b"PolygonGate" => GateKind::Polygon(self.parse_polygon_gate(
                                reader,
                                source,
                                id.clone(),
                                parent_id.clone(),
                            )?),
                            b"EllipsoidGate" => GateKind::Ellipsoid(self.parse_ellipsoid_gate(
                                reader,
                                source,
                                id.clone(),
                                parent_id.clone(),
                            )?),
                            b"BooleanGate" => GateKind::Boolean(self.parse_boolean_gate(
                                reader,
                                source,
                                id.clone(),
                                parent_id.clone(),
                            )?),
                            _ => {
                                skip_element(reader, source, &e)?;
                                continue;
                            }
                        });
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::End(e)
                    if ns_is(&ns, NS_GATING) && e.name().local_name().as_ref() == b"Gate" =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading <Gate>",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        let gate = parsed_gate.ok_or_else(|| {
            FlowGateError::InvalidGate("Gate block does not contain a supported gate".to_string())
        })?;
        self.gates.insert(id, gate);
        Ok(())
    }

    fn parse_standalone_gate(
        &mut self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<(), FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let gate_name = String::from_utf8_lossy(start.local_name().as_ref()).to_string();
        let id =
            GateId::from(required_attr(&attrs, "id", &[NS_GATING, ""], &gate_name)?.to_string());
        let parent_id = optional_attr(&attrs, "parent_id", &[NS_GATING, ""])
            .map(|s| GateId::from(s.to_string()));

        match start.local_name().as_ref() {
            b"RectangleGate" => {
                let gate = self.parse_rectangle_gate(reader, source, id.clone(), parent_id)?;
                self.gates.insert(id, GateKind::Rectangle(gate));
            }
            b"PolygonGate" => {
                let gate = self.parse_polygon_gate(reader, source, id.clone(), parent_id)?;
                self.gates.insert(id, GateKind::Polygon(gate));
            }
            b"EllipsoidGate" => {
                let gate = self.parse_ellipsoid_gate(reader, source, id.clone(), parent_id)?;
                self.gates.insert(id, GateKind::Ellipsoid(gate));
            }
            b"BooleanGate" => {
                let gate = self.parse_boolean_gate(reader, source, id.clone(), parent_id)?;
                self.gates.insert(id, GateKind::Boolean(gate));
            }
            b"QuadrantGate" => {
                for (qid, gate) in self.parse_quadrant_gate(reader, source, parent_id)? {
                    self.gates.insert(qid, gate);
                }
            }
            _ => {
                skip_element(reader, source, start)?;
            }
        }

        Ok(())
    }

    fn parse_quadrant_gate(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        parent_id: Option<GateId>,
    ) -> Result<Vec<(GateId, GateKind)>, FlowGateError> {
        let mut dividers: HashMap<String, RectangleDimension> = HashMap::new();
        let mut divider_values: HashMap<String, Vec<f64>> = HashMap::new();
        let mut quadrants: Vec<(GateId, Vec<(String, f64)>)> = Vec::new();
        let mut buf = Vec::new();

        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"divider") {
                        let (divider_id, dim, values) =
                            self.parse_quadrant_divider(reader, source, &e)?;
                        dividers.insert(divider_id.clone(), dim);
                        divider_values.insert(divider_id, values);
                    } else if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"Quadrant") {
                        quadrants.push(parse_quadrant_definition(reader, source, &e)?);
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::End(e)
                    if element_is(
                        &ns,
                        e.name().local_name().as_ref(),
                        NS_GATING,
                        b"QuadrantGate",
                    ) =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading QuadrantGate",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        let mut out = Vec::new();
        for (qid, positions) in quadrants {
            let mut dims = Vec::with_capacity(positions.len());
            for (divider_id, location) in positions {
                let base_dim = dividers.get(&divider_id).ok_or_else(|| {
                    FlowGateError::InvalidGate(format!(
                        "Quadrant '{}' references unknown divider '{}'",
                        qid, divider_id
                    ))
                })?;
                let values = divider_values.get(&divider_id).ok_or_else(|| {
                    FlowGateError::InvalidGate(format!("Divider '{}' has no values", divider_id))
                })?;
                let (min, max) = interval_for_location(values, location)?;
                dims.push(RectangleDimension {
                    parameter: base_dim.parameter.clone(),
                    transform: base_dim.transform,
                    min,
                    max,
                });
            }
            let gate = RectangleGate::new(qid.clone(), parent_id.clone(), dims)?;
            out.push((qid, GateKind::Rectangle(gate)));
        }

        Ok(out)
    }

    fn parse_quadrant_divider(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<(String, RectangleDimension, Vec<f64>), FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let divider_id = required_attr(&attrs, "id", &[NS_GATING, ""], "divider")?.to_string();
        let transform = parse_transform_ref(&attrs, &self.transforms)?;
        let compensation_ref = optional_attr(&attrs, "compensation-ref", &[NS_GATING, ""])
            .unwrap_or("uncompensated")
            .to_string();
        let mut selector: Option<DimensionSelector> = None;
        let mut values = Vec::<f64>::new();
        let mut buf = Vec::new();
        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if ns_is(&ns, NS_DATATYPE)
                        && matches!(
                            e.local_name().as_ref(),
                            b"parameter" | b"fcs-dimension" | b"new-dimension"
                        )
                    {
                        selector = Some(parse_dimension_selector(reader, source, &e)?);
                        skip_element(reader, source, &e)?;
                    } else if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"value") {
                        values.push(parse_value_content(reader, source)?);
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::Empty(e) => {
                    if ns_is(&ns, NS_DATATYPE)
                        && matches!(
                            e.local_name().as_ref(),
                            b"parameter" | b"fcs-dimension" | b"new-dimension"
                        )
                    {
                        selector = Some(parse_dimension_selector(reader, source, &e)?);
                    } else if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"value") {
                        let value_attrs = attrs_map(reader, source, &e)?;
                        values.push(parse_required_f64_attr(
                            &value_attrs,
                            "value",
                            &[NS_DATATYPE, "", NS_GATING],
                            "value",
                        )?);
                    }
                }
                Event::End(e)
                    if element_is(&ns, e.name().local_name().as_ref(), NS_GATING, b"divider") =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading divider",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }
        let selector = selector.ok_or_else(|| {
            FlowGateError::InvalidGate(format!(
                "Divider '{divider_id}' is missing dimension reference"
            ))
        })?;
        let parameter = dimension_selector_to_parameter(&compensation_ref, selector);
        Ok((
            divider_id,
            RectangleDimension {
                parameter,
                transform,
                min: None,
                max: None,
            },
            values,
        ))
    }

    fn parse_rectangle_gate(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        id: GateId,
        parent_id: Option<GateId>,
    ) -> Result<RectangleGate, FlowGateError> {
        let mut dimensions = Vec::new();
        let mut buf = Vec::new();
        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"dimension") {
                        dimensions.push(self.parse_rectangle_dimension(reader, source, &e)?);
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::End(e)
                    if element_is(
                        &ns,
                        e.name().local_name().as_ref(),
                        NS_GATING,
                        b"RectangleGate",
                    ) =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading RectangleGate",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }
        RectangleGate::new(id, parent_id, dimensions)
    }

    fn parse_rectangle_dimension(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<RectangleDimension, FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let min = parse_optional_f64_attr(&attrs, "min", &[NS_GATING, ""], "dimension")?;
        let max = parse_optional_f64_attr(&attrs, "max", &[NS_GATING, ""], "dimension")?;
        let transform = parse_transform_ref(&attrs, &self.transforms)?;
        let parameter = parse_dimension_ref(
            reader,
            source,
            &attrs,
            "rectangle dimension",
            NS_GATING,
            b"dimension",
        )?;
        Ok(RectangleDimension {
            parameter,
            transform,
            min,
            max,
        })
    }

    fn parse_polygon_gate(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        id: GateId,
        parent_id: Option<GateId>,
    ) -> Result<PolygonGate, FlowGateError> {
        let mut dimensions = Vec::new();
        let mut vertices = Vec::new();
        let mut buf = Vec::new();

        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"dimension") {
                        dimensions.push(self.parse_polygon_dimension(reader, source, &e)?);
                    } else if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"vertex") {
                        vertices.push(parse_vertex(reader, source)?);
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::End(e)
                    if element_is(
                        &ns,
                        e.name().local_name().as_ref(),
                        NS_GATING,
                        b"PolygonGate",
                    ) =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading PolygonGate",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        if dimensions.len() != 2 {
            return Err(FlowGateError::InvalidGate(format!(
                "PolygonGate requires exactly 2 dimensions, found {}",
                dimensions.len()
            )));
        }

        PolygonGate::new(
            id,
            parent_id,
            dimensions.remove(0),
            dimensions.remove(0),
            vertices,
        )
    }

    fn parse_polygon_dimension(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<PolygonDimension, FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let transform = parse_transform_ref(&attrs, &self.transforms)?;
        let parameter = parse_dimension_ref(
            reader,
            source,
            &attrs,
            "polygon dimension",
            NS_GATING,
            b"dimension",
        )?;
        Ok(PolygonDimension {
            parameter,
            transform,
        })
    }

    fn parse_ellipsoid_gate(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        id: GateId,
        parent_id: Option<GateId>,
    ) -> Result<EllipsoidGate, FlowGateError> {
        let mut dimensions = Vec::new();
        let mut mean = Vec::new();
        let mut covariance_upper = Vec::new();
        let mut distance_sq: Option<f64> = None;
        let mut buf = Vec::new();

        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"dimension") {
                        dimensions.push(self.parse_ellipsoid_dimension(reader, source, &e)?);
                    } else if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"mean") {
                        mean = parse_coordinates_block(reader, source, NS_GATING, b"mean")?;
                    } else if element_is(
                        &ns,
                        e.local_name().as_ref(),
                        NS_GATING,
                        b"covarianceMatrix",
                    ) {
                        covariance_upper = parse_covariance_block(reader, source)?;
                    } else if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"distanceSquare")
                    {
                        let attrs = attrs_map(reader, source, &e)?;
                        distance_sq = Some(parse_required_f64_attr(
                            &attrs,
                            "value",
                            &[NS_DATATYPE, ""],
                            "distanceSquare",
                        )?);
                        skip_element(reader, source, &e)?;
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::Empty(e)
                    if element_is(&ns, e.local_name().as_ref(), NS_GATING, b"distanceSquare") =>
                {
                    let attrs = attrs_map(reader, source, &e)?;
                    distance_sq = Some(parse_required_f64_attr(
                        &attrs,
                        "value",
                        &[NS_DATATYPE, ""],
                        "distanceSquare",
                    )?);
                }
                Event::End(e)
                    if element_is(
                        &ns,
                        e.name().local_name().as_ref(),
                        NS_GATING,
                        b"EllipsoidGate",
                    ) =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading EllipsoidGate",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        let distance_sq = distance_sq.ok_or_else(|| {
            FlowGateError::MissingAttribute("value".to_string(), "distanceSquare".to_string())
        })?;
        let n = dimensions.len();
        if covariance_upper.len() == n * n {
            EllipsoidGate::new_general_covariance(
                id,
                parent_id,
                dimensions,
                mean,
                &covariance_upper,
                distance_sq,
            )
        } else {
            EllipsoidGate::new(
                id,
                parent_id,
                dimensions,
                mean,
                &covariance_upper,
                distance_sq,
            )
        }
    }

    fn parse_ellipsoid_dimension(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        start: &BytesStart<'_>,
    ) -> Result<EllipsoidDimension, FlowGateError> {
        let attrs = attrs_map(reader, source, start)?;
        let transform = parse_transform_ref(&attrs, &self.transforms)?;
        let parameter = parse_dimension_ref(
            reader,
            source,
            &attrs,
            "ellipsoid dimension",
            NS_GATING,
            b"dimension",
        )?;
        Ok(EllipsoidDimension {
            parameter,
            transform,
        })
    }

    fn parse_boolean_gate(
        &self,
        reader: &mut NsReader<&[u8]>,
        source: &str,
        current_gate_id: GateId,
        parent_id: Option<GateId>,
    ) -> Result<BooleanGate, FlowGateError> {
        let mut op: Option<BooleanOp> = None;
        let mut operands: Vec<BooleanOperand> = Vec::new();
        let mut buf = Vec::new();

        loop {
            let (ns, event) = match reader.read_resolved_event_into(&mut buf) {
                Ok(v) => v,
                Err(e) => return Err(xml_err(reader, source, format!("XML read error: {e}"))),
            };
            match event {
                Event::Start(e) => {
                    if ns_is(&ns, NS_GATING)
                        && matches!(e.local_name().as_ref(), b"and" | b"or" | b"not")
                    {
                        let (parsed_op, parsed_operands) = parse_boolean_op_block(
                            reader,
                            source,
                            e.local_name().as_ref(),
                            &current_gate_id,
                            &self.gates,
                        )?;
                        op = Some(parsed_op);
                        operands = parsed_operands;
                    } else {
                        skip_element(reader, source, &e)?;
                    }
                }
                Event::End(e)
                    if element_is(
                        &ns,
                        e.name().local_name().as_ref(),
                        NS_GATING,
                        b"BooleanGate",
                    ) =>
                {
                    break;
                }
                Event::Eof => {
                    return Err(xml_err(
                        reader,
                        source,
                        "Unexpected EOF while reading BooleanGate",
                    ));
                }
                _ => {}
            }
            buf.clear();
        }

        let op = op.ok_or_else(|| {
            FlowGateError::InvalidGate("BooleanGate missing required boolean operator".to_string())
        })?;
        BooleanGate::new(current_gate_id, parent_id, op, operands)
    }
}
