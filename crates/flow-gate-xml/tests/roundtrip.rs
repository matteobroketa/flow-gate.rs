use flow_gate_core::{gate::GateKind, EventMatrix, GateId, ParameterName};
use flow_gate_xml::FlowGateDocument;

#[test]
fn parse_and_serialize_simple_rectangle_gate() {
    let xml = r#"
<gating:GatingML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:transforms="http://www.isac-net.org/std/Gating-ML/v2.0/transformations"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <gating:Gate gating:id="G1">
    <gating:RectangleGate>
      <gating:dimension gating:min="0" gating:max="10">
        <data-type:parameter data-type:name="FSC-A"/>
      </gating:dimension>
    </gating:RectangleGate>
  </gating:Gate>
</gating:GatingML>
"#;

    let doc = FlowGateDocument::parse_str(xml).expect("parse");
    let out_xml = doc.to_xml().expect("serialize");
    let reparsed = FlowGateDocument::parse_str(&out_xml).expect("reparse");

    let gate = reparsed
        .gate_registry
        .get(&GateId::from("G1"))
        .expect("gate present");
    assert!(matches!(gate, GateKind::Rectangle(_)));
}

#[test]
fn classify_simple_document() {
    let xml = r#"
<gating:GatingML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:transforms="http://www.isac-net.org/std/Gating-ML/v2.0/transformations"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <gating:Gate gating:id="G1">
    <gating:RectangleGate>
      <gating:dimension gating:min="0" gating:max="10">
        <data-type:parameter data-type:name="FSC-A"/>
      </gating:dimension>
    </gating:RectangleGate>
  </gating:Gate>
</gating:GatingML>
"#;
    let doc = FlowGateDocument::parse_str(xml).expect("parse");
    let matrix = EventMatrix::from_columns(
        vec![vec![-1.0, 2.0, 11.0, 5.0]],
        vec![ParameterName::from("FSC-A")],
    )
    .expect("matrix");
    let results = doc.classify(&matrix).expect("classify");
    let bits = results.get(&GateId::from("G1")).expect("gate result");
    assert_eq!(bits.len(), 4);
    assert!(!bits[0]);
    assert!(bits[1]);
    assert!(!bits[2]);
    assert!(bits[3]);
}

#[test]
fn parser_accepts_nonstandard_prefixes_by_uri() {
    let xml = r#"
<g:GatingML
  xmlns:g="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:t="http://www.isac-net.org/std/Gating-ML/v2.0/transformations"
  xmlns:d="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <g:Gate g:id="G1">
    <g:RectangleGate>
      <g:dimension g:min="0" g:max="10">
        <d:parameter d:name="FSC-A"/>
      </g:dimension>
    </g:RectangleGate>
  </g:Gate>
</g:GatingML>
"#;
    let doc = FlowGateDocument::parse_str(xml).expect("parse");
    assert!(doc.gate_registry.get(&GateId::from("G1")).is_some());
}

#[test]
fn parser_rejects_wrong_gate_namespace() {
    let xml = r#"
<gating:GatingML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:bad="http://example.com/not-gating"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <bad:Gate bad:id="G1">
    <bad:RectangleGate>
      <bad:dimension bad:min="0" bad:max="10">
        <data-type:parameter data-type:name="FSC-A"/>
      </bad:dimension>
    </bad:RectangleGate>
  </bad:Gate>
</gating:GatingML>
"#;
    let doc = FlowGateDocument::parse_str(xml).expect("parse");
    assert!(doc.gate_registry.topological_order().is_empty());
}

#[test]
fn parser_errors_on_missing_required_attributes() {
    let xml_missing_gate_id = r#"
<gating:GatingML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <gating:Gate>
    <gating:RectangleGate>
      <gating:dimension gating:min="0" gating:max="10">
        <data-type:parameter data-type:name="FSC-A"/>
      </gating:dimension>
    </gating:RectangleGate>
  </gating:Gate>
</gating:GatingML>
"#;
    let err = match FlowGateDocument::parse_str(xml_missing_gate_id) {
        Ok(_) => panic!("expected parse error"),
        Err(err) => err,
    };
    assert!(format!("{err}").contains("Missing required XML attribute 'id'"));

    let xml_missing_param_name = r#"
<gating:GatingML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <gating:Gate gating:id="G1">
    <gating:RectangleGate>
      <gating:dimension gating:min="0" gating:max="10">
        <data-type:parameter/>
      </gating:dimension>
    </gating:RectangleGate>
  </gating:Gate>
</gating:GatingML>
"#;
    let err = match FlowGateDocument::parse_str(xml_missing_param_name) {
        Ok(_) => panic!("expected parse error"),
        Err(err) => err,
    };
    assert!(format!("{err}").contains("Missing required XML attribute 'name'"));
}

#[test]
fn roundtrip_preserves_classification_results() {
    let xml = r#"
<gating:GatingML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:transforms="http://www.isac-net.org/std/Gating-ML/v2.0/transformations"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <gating:Gate gating:id="R1">
    <gating:RectangleGate>
      <gating:dimension gating:min="0" gating:max="10">
        <data-type:parameter data-type:name="FSC-A"/>
      </gating:dimension>
    </gating:RectangleGate>
  </gating:Gate>
  <gating:Gate gating:id="R2" gating:parent_id="R1">
    <gating:RectangleGate>
      <gating:dimension gating:min="2" gating:max="8">
        <data-type:parameter data-type:name="FSC-A"/>
      </gating:dimension>
    </gating:RectangleGate>
  </gating:Gate>
</gating:GatingML>
"#;

    let doc = FlowGateDocument::parse_str(xml).expect("parse");
    let matrix = EventMatrix::from_columns(
        vec![vec![-1.0, 1.0, 3.0, 9.0, 11.0]],
        vec![ParameterName::from("FSC-A")],
    )
    .expect("matrix");
    let before = doc.classify(&matrix).expect("classify before");

    let xml_out = doc.to_xml().expect("serialize");
    let doc_roundtrip = FlowGateDocument::parse_str(&xml_out).expect("reparse");
    let after = doc_roundtrip.classify(&matrix).expect("classify after");

    assert_eq!(
        before.get(&GateId::from("R1")).map(|b| b.clone()),
        after.get(&GateId::from("R1")).map(|b| b.clone())
    );
    assert_eq!(
        before.get(&GateId::from("R2")).map(|b| b.clone()),
        after.get(&GateId::from("R2")).map(|b| b.clone())
    );
}

#[test]
fn parser_accepts_official_ellipsoid3d_full_matrix_and_roundtrips() {
    let xml = r#"
<gating:Gating-ML
  xmlns:gating="http://www.isac-net.org/std/Gating-ML/v2.0/gating"
  xmlns:transforms="http://www.isac-net.org/std/Gating-ML/v2.0/transformations"
  xmlns:data-type="http://www.isac-net.org/std/Gating-ML/v2.0/datatypes">
  <gating:EllipsoidGate gating:id="Ellipsoid3D">
    <gating:dimension gating:compensation-ref="FCS">
      <data-type:fcs-dimension data-type:name="FL3-H" />
    </gating:dimension>
    <gating:dimension gating:compensation-ref="FCS">
      <data-type:fcs-dimension data-type:name="FL4-H" />
    </gating:dimension>
    <gating:dimension gating:compensation-ref="FCS">
      <data-type:fcs-dimension data-type:name="FL1-H" />
    </gating:dimension>
    <gating:mean>
      <gating:coordinate data-type:value="40.3" />
      <gating:coordinate data-type:value="30.6" />
      <gating:coordinate data-type:value="20.8" />
    </gating:mean>
    <gating:covarianceMatrix>
      <gating:row>
        <gating:entry data-type:value="2.5" />
        <gating:entry data-type:value="7.5" />
        <gating:entry data-type:value="17.5" />
      </gating:row>
      <gating:row>
        <gating:entry data-type:value="7.5" />
        <gating:entry data-type:value="7" />
        <gating:entry data-type:value="13.5" />
      </gating:row>
      <gating:row>
        <gating:entry data-type:value="15.5" />
        <gating:entry data-type:value="13.5" />
        <gating:entry data-type:value="4.3" />
      </gating:row>
    </gating:covarianceMatrix>
    <gating:distanceSquare data-type:value="1" />
  </gating:EllipsoidGate>
</gating:Gating-ML>
"#;

    let doc = FlowGateDocument::parse_str(xml).expect("parse");
    let matrix = EventMatrix::from_columns(
        vec![vec![40.3, 60.3], vec![30.6, 50.6], vec![20.8, 40.8]],
        vec![
            ParameterName::from("FL3-H"),
            ParameterName::from("FL4-H"),
            ParameterName::from("FL1-H"),
        ],
    )
    .expect("matrix");
    let results = doc.classify(&matrix).expect("classify");
    let bits = results
        .get(&GateId::from("Ellipsoid3D"))
        .expect("ellipsoid result");
    assert_eq!(bits.len(), 2);
    assert!(bits[0]);
    assert!(!bits[1]);

    let xml_out = doc.to_xml().expect("serialize");
    let reparsed = FlowGateDocument::parse_str(&xml_out).expect("reparse");
    let after = reparsed.classify(&matrix).expect("reclassify");
    assert_eq!(
        results.get(&GateId::from("Ellipsoid3D")).map(|b| b.clone()),
        after.get(&GateId::from("Ellipsoid3D")).map(|b| b.clone())
    );
}
