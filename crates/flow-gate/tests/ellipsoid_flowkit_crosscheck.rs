use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use flow_fcs::{
    keyword::{FloatKeyword, IntegerableKeyword, MixedKeyword, StringableKeyword},
    Fcs, Keyword,
};
use flow_gate::{EventMatrix, FlowGateDocument, GateId, ParameterName, SpectrumMatrixSpec};

#[test]
fn set1_ellipse_and_ellipsoid_vectors_match_expected_when_data_available() {
    let Some(root) = compliance_root() else {
        eprintln!("SKIP: GATING_ML_COMPLIANCE_ROOT not set");
        return;
    };
    let set_dir = root.join("set 1");
    let xml_path = set_dir.join("gates1.xml");
    let fcs_path = set_dir.join("data1.fcs");
    if !xml_path.exists() || !fcs_path.exists() {
        eprintln!(
            "SKIP: compliance files not found under {}",
            set_dir.display()
        );
        return;
    }

    let xml = fs::read_to_string(&xml_path).expect("read gates1.xml");
    let doc = FlowGateDocument::parse_str(&xml).expect("parse set1 xml");

    let fcs = Fcs::open(path_to_str(&fcs_path).expect("utf8 path")).expect("open data1.fcs");
    let (matrix, fcs_comp) = build_scaled_event_matrix(&fcs).expect("build event matrix");
    let results = doc
        .classify_with_fcs_compensation(&matrix, fcs_comp.as_ref())
        .expect("classify set1");

    for gate_name in ["Ellipse1", "Ellipsoid3D"] {
        let gate_id = GateId::from(gate_name);
        let actual = results
            .get(&gate_id)
            .unwrap_or_else(|| panic!("missing gate result '{}'", gate_name));
        let expected_path = set_dir.join(format!("Results_{gate_name}.txt"));
        let expected = load_expected_bits(&expected_path).expect("load expected bits");
        assert_eq!(
            actual.len(),
            expected.len(),
            "length mismatch for {}",
            gate_name
        );
        for idx in 0..expected.len() {
            assert_eq!(
                actual[idx], expected[idx],
                "mismatch for {} at event {}",
                gate_name, idx
            );
        }
    }
}

#[test]
fn flowkit_crosscheck_for_ellipse_and_ellipsoid_when_available() {
    let Some(root) = compliance_root() else {
        eprintln!("SKIP: GATING_ML_COMPLIANCE_ROOT not set");
        return;
    };
    let set_dir = root.join("set 1");
    let xml_path = set_dir.join("gates1.xml");
    let fcs_path = set_dir.join("data1.fcs");
    if !xml_path.exists() || !fcs_path.exists() {
        eprintln!(
            "SKIP: compliance files not found under {}",
            set_dir.display()
        );
        return;
    }

    let flowkit_check = Command::new("python")
        .args(["-c", "import flowkit"])
        .status();
    let Ok(status) = flowkit_check else {
        eprintln!("SKIP: python not available for FlowKit cross-check");
        return;
    };
    if !status.success() {
        eprintln!("SKIP: python FlowKit module not available");
        return;
    }

    let temp = std::env::temp_dir().join(format!(
        "flow_gate_flowkit_crosscheck_{}",
        std::process::id()
    ));
    fs::create_dir_all(&temp).expect("create temp dir");
    let out_ellipse = temp.join("flowkit_ellipse1.txt");
    let out_ellipsoid = temp.join("flowkit_ellipsoid3d.txt");

    let script = r#"
import sys
import numpy as np
import flowkit as fk
from pathlib import Path

root = Path(sys.argv[1])
out_ellipse = Path(sys.argv[2])
out_ellipsoid = Path(sys.argv[3])
set_dir = root / "set 1"
xml_path = set_dir / "gates1.xml"
fcs_path = set_dir / "data1.fcs"

session = fk.Session(gating_strategy=str(xml_path))
sample = fk.Sample(str(fcs_path), sample_id="data1")
session.add_samples(sample)
session.analyze_samples()

ellipse = session.get_gate_membership("data1", "Ellipse1")
ellipsoid = session.get_gate_membership("data1", "Ellipsoid3D")
np.savetxt(out_ellipse, ellipse.astype(np.int8), fmt="%d")
np.savetxt(out_ellipsoid, ellipsoid.astype(np.int8), fmt="%d")
"#;
    let status = Command::new("python")
        .arg("-c")
        .arg(script)
        .arg(path_to_str(&root).expect("utf8 root"))
        .arg(path_to_str(&out_ellipse).expect("utf8 out_ellipse"))
        .arg(path_to_str(&out_ellipsoid).expect("utf8 out_ellipsoid"))
        .status()
        .expect("run python flowkit cross-check");
    assert!(status.success(), "FlowKit cross-check script failed");

    let xml = fs::read_to_string(&xml_path).expect("read gates1.xml");
    let doc = FlowGateDocument::parse_str(&xml).expect("parse set1 xml");
    let fcs = Fcs::open(path_to_str(&fcs_path).expect("utf8 path")).expect("open data1.fcs");
    let (matrix, fcs_comp) = build_scaled_event_matrix(&fcs).expect("build event matrix");
    let results = doc
        .classify_with_fcs_compensation(&matrix, fcs_comp.as_ref())
        .expect("classify set1");

    let rust_ellipse = results
        .get(&GateId::from("Ellipse1"))
        .expect("missing Rust Ellipse1");
    let rust_ellipsoid = results
        .get(&GateId::from("Ellipsoid3D"))
        .expect("missing Rust Ellipsoid3D");

    let py_ellipse = load_expected_bits(&out_ellipse).expect("load flowkit ellipse file");
    let py_ellipsoid = load_expected_bits(&out_ellipsoid).expect("load flowkit ellipsoid file");

    assert_eq!(
        rust_ellipse.len(),
        py_ellipse.len(),
        "Ellipse1 length mismatch"
    );
    assert_eq!(
        rust_ellipsoid.len(),
        py_ellipsoid.len(),
        "Ellipsoid3D length mismatch"
    );
    for i in 0..py_ellipse.len() {
        assert_eq!(rust_ellipse[i], py_ellipse[i], "Ellipse1 mismatch at {}", i);
    }
    for i in 0..py_ellipsoid.len() {
        assert_eq!(
            rust_ellipsoid[i], py_ellipsoid[i],
            "Ellipsoid3D mismatch at {}",
            i
        );
    }
}

#[test]
fn set1_range1_and_ratio_gate_vectors_match_expected_when_data_available() {
    let Some(root) = compliance_root() else {
        eprintln!("SKIP: GATING_ML_COMPLIANCE_ROOT not set");
        return;
    };
    let set_dir = root.join("set 1");
    let xml_path = set_dir.join("gates1.xml");
    let fcs_path = set_dir.join("data1.fcs");
    if !xml_path.exists() || !fcs_path.exists() {
        eprintln!(
            "SKIP: compliance files not found under {}",
            set_dir.display()
        );
        return;
    }

    let xml = fs::read_to_string(&xml_path).expect("read gates1.xml");
    let doc = FlowGateDocument::parse_str(&xml).expect("parse set1 xml");

    let fcs = Fcs::open(path_to_str(&fcs_path).expect("utf8 path")).expect("open data1.fcs");
    let (matrix, fcs_comp) = build_scaled_event_matrix(&fcs).expect("build event matrix");
    let results = doc
        .classify_with_fcs_compensation(&matrix, fcs_comp.as_ref())
        .expect("classify set1");

    for gate_name in ["Range1", "And3", "And4", "Or1", "ParAnd3", "RatRange2"] {
        let gate_id = GateId::from(gate_name);
        let actual = results
            .get(&gate_id)
            .unwrap_or_else(|| panic!("missing gate result '{}'", gate_name));
        let expected_path = set_dir.join(format!("Results_{gate_name}.txt"));
        let expected = load_expected_bits(&expected_path).expect("load expected bits");
        assert_eq!(
            actual.len(),
            expected.len(),
            "length mismatch for {}",
            gate_name
        );
        for idx in 0..expected.len() {
            assert_eq!(
                actual[idx], expected[idx],
                "mismatch for {} at event {}",
                gate_name, idx
            );
        }
    }
}

fn compliance_root() -> Option<PathBuf> {
    std::env::var_os("GATING_ML_COMPLIANCE_ROOT").map(PathBuf::from)
}

fn build_scaled_event_matrix(
    fcs: &Fcs,
) -> anyhow::Result<(EventMatrix, Option<SpectrumMatrixSpec>)> {
    let n_params = *fcs
        .metadata
        .get_number_of_parameters()
        .map_err(|e| anyhow::anyhow!("Missing $PAR in FCS metadata: {e}"))?;

    let mut columns: Vec<Vec<f64>> = Vec::with_capacity(n_params);
    let mut names: Vec<ParameterName> = Vec::with_capacity(n_params);

    for idx in 1..=n_params {
        let channel_name = fcs
            .metadata
            .get_parameter_channel_name(idx)
            .map_err(|e| anyhow::anyhow!("Missing $P{}N in FCS metadata: {e}", idx))?
            .to_string();
        let raw = fcs
            .get_parameter_events_slice(&channel_name)
            .map_err(|e| anyhow::anyhow!("Missing data column '{}': {e}", channel_name))?;

        let (range, decades, offset, gain) = scale_params_for_channel(fcs, idx)?;
        let mut scaled = Vec::with_capacity(raw.len());
        for value in raw {
            scaled.push(channel_to_scale(
                *value as f64,
                range,
                decades,
                offset,
                gain,
            ));
        }

        columns.push(scaled);
        names.push(ParameterName::from(channel_name));
    }

    let matrix = EventMatrix::from_columns(columns, names)?;
    let comp = build_fcs_compensation_spec(fcs)?;
    Ok((matrix, comp))
}

fn scale_params_for_channel(
    fcs: &Fcs,
    channel_idx_1_based: usize,
) -> anyhow::Result<(f64, f64, f64, f64)> {
    let key_prefix = format!("$P{channel_idx_1_based}");
    let range = fcs
        .metadata
        .get_parameter_numeric_metadata(channel_idx_1_based, "R")
        .map_err(|e| anyhow::anyhow!("Missing {}R: {e}", key_prefix))?
        .get_usize();
    let range = (*range).max(1) as f64;

    let gain = match fcs.metadata.get_float_keyword(&format!("{key_prefix}G")) {
        Ok(FloatKeyword::PnG(v)) if *v > 0.0 && v.is_finite() => parse_losslessish_f32(*v),
        _ => 1.0,
    };

    let (decades, offset) = match fcs.metadata.keywords.get(&format!("{key_prefix}E")) {
        Some(Keyword::Mixed(MixedKeyword::PnE(f1, f2))) => {
            (parse_losslessish_f32(*f1), parse_losslessish_f32(*f2))
        }
        Some(Keyword::String(raw)) => {
            parse_pne_string(raw.get_str().as_ref()).unwrap_or((0.0, 0.0))
        }
        _ => (0.0, 0.0),
    };

    Ok((range, decades, offset, gain))
}

fn parse_pne_string(raw: &str) -> Option<(f64, f64)> {
    let mut parts = raw.split(',').map(str::trim);
    let p1 = parts.next()?.parse::<f64>().ok()?;
    let p2 = parts.next()?.parse::<f64>().ok()?;
    Some((p1, p2))
}

fn parse_losslessish_f32(value: f32) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(value as f64)
}

fn channel_to_scale(raw: f64, range: f64, decades: f64, offset: f64, gain: f64) -> f64 {
    if !raw.is_finite() {
        return f64::NAN;
    }

    if decades > 0.0 {
        let true_offset = if offset == 0.0 { 1.0 } else { offset };
        true_offset * 10.0_f64.powf((decades * raw) / range)
    } else {
        let true_gain = if gain > 0.0 { gain } else { 1.0 };
        raw / true_gain
    }
}

fn build_fcs_compensation_spec(fcs: &Fcs) -> anyhow::Result<Option<SpectrumMatrixSpec>> {
    let Some((matrix, channel_refs)) = fcs
        .get_spillover_matrix()
        .map_err(|e| anyhow::anyhow!("Failed to parse spillover matrix: {e}"))?
    else {
        return Ok(None);
    };

    let resolved = resolve_spillover_channel_names(fcs, &channel_refs)?;
    let n_rows = matrix.nrows();
    let n_cols = matrix.ncols();
    if n_rows == 0 || n_cols == 0 {
        return Ok(None);
    }

    let mut coefficients = Vec::with_capacity(n_rows * n_cols);
    for r in 0..n_rows {
        for c in 0..n_cols {
            coefficients.push(matrix[(r, c)] as f64);
        }
    }

    let fluorochromes: Vec<ParameterName> = resolved
        .iter()
        .map(|name| ParameterName::from(name.clone()))
        .collect();
    let detectors: Vec<ParameterName> = resolved
        .iter()
        .map(|name| ParameterName::from(name.clone()))
        .collect();

    Ok(Some(SpectrumMatrixSpec {
        id: "FCS".to_string(),
        fluorochromes,
        detectors,
        coefficients,
        matrix_inverted_already: false,
    }))
}

fn resolve_spillover_channel_names(fcs: &Fcs, refs: &[String]) -> anyhow::Result<Vec<String>> {
    let n_params = *fcs
        .metadata
        .get_number_of_parameters()
        .map_err(|e| anyhow::anyhow!("Missing $PAR in FCS metadata: {e}"))?;
    let mut known_channels = HashSet::<String>::with_capacity(n_params);
    for idx in 1..=n_params {
        let name = fcs
            .metadata
            .get_parameter_channel_name(idx)
            .map_err(|e| anyhow::anyhow!("Missing $P{}N in FCS metadata: {e}", idx))?;
        known_channels.insert(name.to_string());
    }

    let mut resolved = Vec::with_capacity(refs.len());
    for raw in refs {
        if known_channels.contains(raw) {
            resolved.push(raw.clone());
            continue;
        }
        if let Ok(index) = raw.parse::<usize>() {
            let channel_name = fcs
                .metadata
                .get_parameter_channel_name(index)
                .map_err(|e| {
                    anyhow::anyhow!("Invalid spillover channel reference '{}': {e}", raw)
                })?;
            resolved.push(channel_name.to_string());
            continue;
        }
        anyhow::bail!(
            "Unresolvable spillover channel reference '{}': not a channel name and not a parameter index",
            raw
        );
    }
    Ok(resolved)
}

fn load_expected_bits(path: &Path) -> anyhow::Result<Vec<bool>> {
    let content = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in content.lines() {
        let token = line.trim();
        if token.is_empty() {
            continue;
        }
        match token {
            "0" => out.push(false),
            "1" => out.push(true),
            _ => anyhow::bail!("Invalid token '{}' in {}", token, path.display()),
        }
    }
    Ok(out)
}

fn path_to_str(path: &Path) -> Option<&str> {
    path.to_str()
}
