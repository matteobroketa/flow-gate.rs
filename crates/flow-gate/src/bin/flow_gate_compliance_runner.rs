use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use flow_fcs::{
    keyword::{FloatKeyword, IntegerableKeyword, MixedKeyword, StringableKeyword},
    Fcs, Keyword,
};
use flow_gate::{
    BitVec, BooleanGate, BooleanOp, EventMatrix, FlowGateDocument, Gate, GateId, GateKind,
    ParameterName, SpectrumMatrixSpec, Transform, TransformKind,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug)]
struct CliArgs {
    root: PathBuf,
    output_json: Option<PathBuf>,
    summary_csv: Option<PathBuf>,
    allow_unmapped_expected: bool,
    only_set: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct SetConfig {
    canonical_name: &'static str,
    display_name: &'static str,
    xml_name: &'static str,
    data_name: &'static str,
}

const SETS: [SetConfig; 2] = [
    SetConfig {
        canonical_name: "set1",
        display_name: "set 1",
        xml_name: "gates1.xml",
        data_name: "data1.fcs",
    },
    SetConfig {
        canonical_name: "set2",
        display_name: "set 2",
        xml_name: "gates2.xml",
        data_name: "data2.fcs",
    },
];

#[derive(Debug, Default)]
struct SummaryIndex {
    expected_alias_by_set: HashMap<String, HashMap<String, String>>,
    events_in_by_set: HashMap<String, HashMap<String, usize>>,
}

#[derive(Debug, Clone)]
enum MappingSource {
    Direct,
    SummaryCsv,
    CaseInsensitive,
    Normalized,
    NormalizedL1,
    UnmatchedSetFallback,
    Unresolved,
}

impl MappingSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::SummaryCsv => "summary_csv",
            Self::CaseInsensitive => "case_insensitive",
            Self::Normalized => "normalized",
            Self::NormalizedL1 => "normalized_l1",
            Self::UnmatchedSetFallback => "unmatched_set_fallback",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Debug)]
struct ResolvedGate {
    resolved_gate_id: Option<String>,
    mapping_source: MappingSource,
}

#[derive(Debug)]
struct GateAliasResolver {
    exact: HashSet<String>,
    lower: HashMap<String, BTreeSet<String>>,
    normalized: HashMap<String, BTreeSet<String>>,
    normalized_l1: HashMap<String, BTreeSet<String>>,
    summary_aliases: HashMap<String, String>,
}

#[derive(Serialize, Debug, Default)]
struct ComplianceReport {
    schema_version: String,
    root: String,
    all_passed: bool,
    checked_sets: usize,
    checked_gates: usize,
    checked_events: usize,
    failed_gates: usize,
    failed_events: usize,
    unknown_expected_gate_mappings: usize,
    sets: Vec<SetReport>,
}

#[derive(Serialize, Debug, Default)]
struct SetReport {
    canonical_name: String,
    display_name: String,
    set_directory: String,
    xml_file: String,
    data_file: String,
    n_events: usize,
    expected_gate_files: usize,
    checked_gates: usize,
    checked_events: usize,
    failed_gates: usize,
    failed_events: usize,
    unknown_expected_gate_ids: Vec<String>,
    unmatched_actual_gate_ids: Vec<String>,
    all_passed: bool,
    gate_results: Vec<GateReport>,
}

#[derive(Serialize, Debug)]
struct GateReport {
    set_name: String,
    expected_gate_id: String,
    expected_file: String,
    resolved_gate_id: Option<String>,
    mapping_source: String,
    gate_type: Option<String>,
    dependency_ancestry: Vec<String>,
    total_events: usize,
    expected_in: usize,
    actual_in: Option<usize>,
    actual_sha256: Option<String>,
    mismatches: usize,
    first_mismatch_at: Option<usize>,
    first_mismatch_indices: Vec<usize>,
    passed: bool,
    summary_events_in: Option<usize>,
    summary_events_in_matches_expected: Option<bool>,
    diagnostics: Option<MismatchDiagnostics>,
    note: Option<String>,
}

#[derive(Serialize, Debug)]
struct MismatchDiagnostics {
    event_index: usize,
    expected_value: bool,
    actual_value: bool,
    pre_parent_value: Option<bool>,
    post_parent_value: Option<bool>,
    parent_gate_id: Option<String>,
    parent_value: Option<bool>,
    compensated_coords: Vec<f64>,
    transformed_coords: Vec<f64>,
}

fn main() -> Result<()> {
    let args = parse_cli_args()?;
    let summary_index = load_summary_index(
        args.summary_csv
            .clone()
            .unwrap_or_else(|| args.root.join("Summary.csv")),
    )?;

    let mut report = ComplianceReport {
        schema_version: "flow_gate.parity_ledger.v1".to_string(),
        root: args.root.display().to_string(),
        ..ComplianceReport::default()
    };

    for set in SETS {
        if !selected_set_matches(args.only_set.as_deref(), set.canonical_name) {
            continue;
        }
        let set_report = run_set(&args.root, set, &summary_index)?;
        report.checked_sets += 1;
        report.checked_gates += set_report.checked_gates;
        report.checked_events += set_report.checked_events;
        report.failed_gates += set_report.failed_gates;
        report.failed_events += set_report.failed_events;
        report.unknown_expected_gate_mappings += set_report.unknown_expected_gate_ids.len();
        report.sets.push(set_report);
    }

    report.all_passed = report.failed_gates == 0
        && report.failed_events == 0
        && (args.allow_unmapped_expected || report.unknown_expected_gate_mappings == 0);

    if let Some(path) = args.output_json {
        let json = serde_json::to_string_pretty(&report)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write JSON report '{}'", path.display()))?;
        println!("Wrote compliance ledger: {}", path.display());
    }

    if !report.all_passed {
        let unknown_clause = if args.allow_unmapped_expected {
            String::new()
        } else {
            format!(
                ", {} unmapped expected gate ids",
                report.unknown_expected_gate_mappings
            )
        };
        bail!(
            "Compliance check failed: {} failed gates, {} failed events{} across {} checked gates",
            report.failed_gates,
            report.failed_events,
            unknown_clause,
            report.checked_gates
        );
    }

    println!(
        "Compliance check passed: {} checked gates, {} checked events, 0 mismatches.",
        report.checked_gates, report.checked_events
    );
    Ok(())
}

fn parse_cli_args() -> Result<CliArgs> {
    let mut root: Option<PathBuf> = None;
    let mut output_json: Option<PathBuf> =
        std::env::var_os("GATING_ML_OUTPUT_JSON").map(PathBuf::from);
    let mut summary_csv: Option<PathBuf> =
        std::env::var_os("GATING_ML_SUMMARY_CSV").map(PathBuf::from);
    let mut allow_unmapped_expected = false;
    let mut only_set: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--root" => {
                let Some(value) = args.next() else {
                    bail!("Missing value for --root");
                };
                root = Some(PathBuf::from(value));
            }
            "--output-json" => {
                let Some(value) = args.next() else {
                    bail!("Missing value for --output-json");
                };
                output_json = Some(PathBuf::from(value));
            }
            "--summary-csv" => {
                let Some(value) = args.next() else {
                    bail!("Missing value for --summary-csv");
                };
                summary_csv = Some(PathBuf::from(value));
            }
            "--allow-unmapped-expected" => {
                allow_unmapped_expected = true;
            }
            "--only-set" => {
                let Some(value) = args.next() else {
                    bail!("Missing value for --only-set");
                };
                let normalized = canonical_set_name(&value);
                if normalized != "set1" && normalized != "set2" {
                    bail!("--only-set must be one of: set1, set2");
                }
                only_set = Some(normalized);
            }
            other if other.starts_with("--") => {
                bail!("Unknown argument '{other}'");
            }
            positional => {
                if root.is_none() {
                    root = Some(PathBuf::from(positional));
                } else {
                    bail!("Unexpected positional argument '{positional}'");
                }
            }
        }
    }

    if root.is_none() {
        root = std::env::var_os("GATING_ML_COMPLIANCE_ROOT").map(PathBuf::from);
    }
    let Some(root) = root else {
        print_usage();
        bail!("Missing compliance root path");
    };

    Ok(CliArgs {
        root,
        output_json,
        summary_csv,
        allow_unmapped_expected,
        only_set,
    })
}

fn print_usage() {
    eprintln!(
        "Usage: flow_gate_compliance_runner [--root <path>] [--output-json <path>] [--summary-csv <path>] [--allow-unmapped-expected]
       [--only-set <set1|set2>]
  or: flow_gate_compliance_runner <path>
  env: GATING_ML_COMPLIANCE_ROOT, GATING_ML_OUTPUT_JSON, GATING_ML_SUMMARY_CSV"
    );
}

fn selected_set_matches(selected: Option<&str>, candidate: &str) -> bool {
    match selected {
        Some(value) => value == candidate,
        None => true,
    }
}

fn run_set(root: &Path, set_cfg: SetConfig, summary_index: &SummaryIndex) -> Result<SetReport> {
    let set_dir = resolve_set_dir(root, set_cfg)?;
    let xml_path = set_dir.join(set_cfg.xml_name);
    let data_path = set_dir.join(set_cfg.data_name);
    let expected_dir = if set_dir.join("expected").is_dir() {
        set_dir.join("expected")
    } else {
        set_dir.clone()
    };

    let xml = fs::read_to_string(&xml_path)
        .with_context(|| format!("Failed to read {}", xml_path.display()))?;
    let doc = FlowGateDocument::parse_str(&xml)
        .with_context(|| format!("Failed to parse {}", xml_path.display()))?;

    let fcs = Fcs::open(path_to_str(&data_path)?)
        .with_context(|| format!("Failed to open {}", data_path.display()))?;
    let (matrix, fcs_comp) = build_scaled_event_matrix(&fcs)?;

    let prepared = doc
        .prepare_owned_matrix_with_fcs_compensation(&matrix, fcs_comp.as_ref())
        .with_context(|| format!("Failed to prepare matrix for {}", set_cfg.display_name))?;
    let results = doc
        .gate_registry
        .classify_all(&prepared)
        .with_context(|| format!("Classification failed for {}", set_cfg.display_name))?;

    let expected_files = collect_expected_files(&expected_dir)?;
    if expected_files.is_empty() {
        bail!("No Results_*.txt files found in {}", expected_dir.display());
    }

    let set_key = canonical_set_name(set_cfg.display_name);
    let summary_aliases = summary_index
        .expected_alias_by_set
        .get(&set_key)
        .cloned()
        .unwrap_or_default();
    let summary_events_in = summary_index
        .events_in_by_set
        .get(&set_key)
        .cloned()
        .unwrap_or_default();
    let resolver = GateAliasResolver::new(results.keys(), summary_aliases);

    let mut set_report = SetReport {
        canonical_name: set_cfg.canonical_name.to_string(),
        display_name: set_cfg.display_name.to_string(),
        set_directory: set_dir.display().to_string(),
        xml_file: xml_path.display().to_string(),
        data_file: data_path.display().to_string(),
        n_events: prepared.n_events,
        expected_gate_files: expected_files.len(),
        ..SetReport::default()
    };

    let mut unresolved_rows: Vec<(PathBuf, String, Vec<bool>)> = Vec::new();
    let mut resolved_actual_ids = HashSet::<String>::new();
    for expected_path in expected_files {
        let expected_gate_id = gate_name_from_expected_path(&expected_path)?;
        let expected_bits = load_expected_bits(&expected_path)?;
        let resolution = resolver.resolve(&expected_gate_id);
        if resolution.resolved_gate_id.is_none() {
            unresolved_rows.push((expected_path, expected_gate_id, expected_bits));
            continue;
        }
        let resolved_gate_id = resolution.resolved_gate_id.expect("checked is_some above");
        let entry = evaluate_expected_against_actual(
            &set_cfg,
            &summary_events_in,
            &doc,
            &prepared,
            &results,
            expected_path,
            expected_gate_id,
            expected_bits,
            resolved_gate_id.clone(),
            resolution.mapping_source,
        )?;
        if entry.resolved_gate_id.is_some() {
            resolved_actual_ids.insert(resolved_gate_id);
        }
        apply_gate_report_to_set_totals(&mut set_report, &entry);
        set_report.gate_results.push(entry);
    }

    let mut unmatched_actual: BTreeSet<String> = results
        .keys()
        .map(|id| id.as_str().to_string())
        .filter(|id| !resolved_actual_ids.contains(id))
        .collect();

    for (expected_path, expected_gate_id, expected_bits) in unresolved_rows {
        let fallback = if unmatched_actual.len() == 1 {
            unmatched_actual.iter().next().cloned()
        } else {
            None
        };
        if let Some(mapped_gate) = fallback.clone() {
            unmatched_actual.remove(&mapped_gate);
            resolved_actual_ids.insert(mapped_gate.clone());
            let entry = evaluate_expected_against_actual(
                &set_cfg,
                &summary_events_in,
                &doc,
                &prepared,
                &results,
                expected_path,
                expected_gate_id,
                expected_bits,
                mapped_gate,
                MappingSource::UnmatchedSetFallback,
            )?;
            apply_gate_report_to_set_totals(&mut set_report, &entry);
            set_report.gate_results.push(entry);
            continue;
        }

        let expected_file = expected_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_string();
        let entry = GateReport {
            set_name: set_cfg.display_name.to_string(),
            expected_gate_id: expected_gate_id.clone(),
            expected_file,
            resolved_gate_id: None,
            mapping_source: MappingSource::Unresolved.as_str().to_string(),
            gate_type: None,
            dependency_ancestry: Vec::new(),
            total_events: expected_bits.len(),
            expected_in: expected_bits.iter().copied().filter(|v| *v).count(),
            actual_in: None,
            actual_sha256: None,
            mismatches: 0,
            first_mismatch_at: None,
            first_mismatch_indices: Vec::new(),
            passed: false,
            summary_events_in: None,
            summary_events_in_matches_expected: None,
            diagnostics: None,
            note: Some("No matching gate id in classification output".to_string()),
        };
        set_report
            .unknown_expected_gate_ids
            .push(expected_gate_id.clone());
        apply_gate_report_to_set_totals(&mut set_report, &entry);
        set_report.gate_results.push(entry);
    }

    let mut unmatched_actual_gate_ids = Vec::new();
    for gate_id in results.keys() {
        let gate_name = gate_id.as_str().to_string();
        if !resolved_actual_ids.contains(&gate_name) {
            unmatched_actual_gate_ids.push(gate_name);
        }
    }
    unmatched_actual_gate_ids.sort();
    set_report.unmatched_actual_gate_ids = unmatched_actual_gate_ids;
    set_report.unknown_expected_gate_ids.sort();
    set_report
        .gate_results
        .sort_by(|a, b| a.expected_gate_id.cmp(&b.expected_gate_id));

    set_report.all_passed =
        set_report.failed_gates == 0 && set_report.unknown_expected_gate_ids.is_empty();

    println!(
        "{}: checked {} gates, failed {} gates ({} events), unknown mappings {}",
        set_cfg.display_name,
        set_report.checked_gates,
        set_report.failed_gates,
        set_report.failed_events,
        set_report.unknown_expected_gate_ids.len()
    );
    Ok(set_report)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_expected_against_actual(
    set_cfg: &SetConfig,
    summary_events_in: &HashMap<String, usize>,
    doc: &FlowGateDocument,
    prepared: &EventMatrix,
    results: &HashMap<GateId, BitVec>,
    expected_path: PathBuf,
    expected_gate_id: String,
    expected_bits: Vec<bool>,
    resolved_gate_id: String,
    mapping_source: MappingSource,
) -> Result<GateReport> {
    let expected_file = expected_path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_string();

    let mut entry = GateReport {
        set_name: set_cfg.display_name.to_string(),
        expected_gate_id,
        expected_file,
        resolved_gate_id: Some(resolved_gate_id.clone()),
        mapping_source: mapping_source.as_str().to_string(),
        gate_type: None,
        dependency_ancestry: Vec::new(),
        total_events: expected_bits.len(),
        expected_in: expected_bits.iter().copied().filter(|v| *v).count(),
        actual_in: None,
        actual_sha256: None,
        mismatches: 0,
        first_mismatch_at: None,
        first_mismatch_indices: Vec::new(),
        passed: false,
        summary_events_in: None,
        summary_events_in_matches_expected: None,
        diagnostics: None,
        note: None,
    };

    let gate_key = GateId::from(resolved_gate_id.clone());
    let Some(actual_bits) = results.get(&gate_key) else {
        entry.note = Some("Resolved gate id missing from results map".to_string());
        return Ok(entry);
    };
    let Some(gate) = doc.gate_registry.get(&gate_key) else {
        entry.note = Some("Resolved gate id missing from gate registry".to_string());
        return Ok(entry);
    };

    entry.gate_type = Some(gate_kind_name(gate).to_string());
    entry.dependency_ancestry = dependency_ancestry(doc, &gate_key);
    entry.summary_events_in = summary_events_in.get(&resolved_gate_id).copied();
    entry.summary_events_in_matches_expected = entry
        .summary_events_in
        .map(|summary_count| summary_count == entry.expected_in);
    entry.actual_in = Some(actual_bits.iter().filter(|b| **b).count());
    entry.actual_sha256 = Some(bools_sha256(actual_bits));

    if actual_bits.len() != expected_bits.len() {
        entry.note = Some(format!(
            "Length mismatch: expected {}, actual {}",
            expected_bits.len(),
            actual_bits.len()
        ));
        entry.mismatches = expected_bits.len().max(actual_bits.len());
        entry.first_mismatch_at = Some(0);
        entry.passed = false;
        return Ok(entry);
    }

    let mut mismatch_indices = Vec::new();
    let mut mismatch_total = 0usize;
    for idx in 0..expected_bits.len() {
        if actual_bits[idx] != expected_bits[idx] {
            mismatch_total += 1;
            if mismatch_indices.len() < 10 {
                mismatch_indices.push(idx);
            }
        }
    }

    entry.mismatches = mismatch_total;
    entry.first_mismatch_at = mismatch_indices.first().copied();
    entry.first_mismatch_indices = mismatch_indices;
    entry.passed = mismatch_total == 0;
    if let Some(first_idx) = entry.first_mismatch_at {
        entry.diagnostics = mismatch_diagnostics(
            gate,
            prepared,
            actual_bits,
            &expected_bits,
            first_idx,
            results,
        );
    }
    Ok(entry)
}

fn apply_gate_report_to_set_totals(set_report: &mut SetReport, entry: &GateReport) {
    if entry.resolved_gate_id.is_none() {
        return;
    }
    set_report.checked_gates += 1;
    set_report.checked_events += entry.total_events;
    if !entry.passed {
        set_report.failed_gates += 1;
        set_report.failed_events += entry.mismatches;
    }
}

fn resolve_set_dir(root: &Path, set_cfg: SetConfig) -> Result<PathBuf> {
    let candidates = [
        root.join(set_cfg.display_name),
        root.join(set_cfg.canonical_name),
        root.join(set_cfg.canonical_name.replace("set", "set ")),
    ];
    for candidate in &candidates {
        if candidate.is_dir() {
            return Ok(candidate.clone());
        }
    }
    bail!(
        "Missing set directory for {}. Tried: {}",
        set_cfg.display_name,
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn build_scaled_event_matrix(fcs: &Fcs) -> Result<(EventMatrix, Option<SpectrumMatrixSpec>)> {
    let n_params = *fcs
        .metadata
        .get_number_of_parameters()
        .context("Missing $PAR in FCS metadata")?;

    let mut columns: Vec<Vec<f64>> = Vec::with_capacity(n_params);
    let mut names: Vec<ParameterName> = Vec::with_capacity(n_params);

    for idx in 1..=n_params {
        let channel_name = fcs
            .metadata
            .get_parameter_channel_name(idx)
            .with_context(|| format!("Missing $P{}N in FCS metadata", idx))?
            .to_string();
        let raw = fcs
            .get_parameter_events_slice(&channel_name)
            .with_context(|| format!("Missing data column '{}'", channel_name))?;

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

    let matrix = EventMatrix::from_columns(columns, names)
        .context("Failed to construct EventMatrix from scaled FCS columns")?;
    let comp = build_fcs_compensation_spec(fcs)?;
    Ok((matrix, comp))
}

fn scale_params_for_channel(fcs: &Fcs, channel_idx_1_based: usize) -> Result<(f64, f64, f64, f64)> {
    let key_prefix = format!("$P{channel_idx_1_based}");
    let range = fcs
        .metadata
        .get_parameter_numeric_metadata(channel_idx_1_based, "R")
        .with_context(|| format!("Missing {}R", key_prefix))?
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

fn build_fcs_compensation_spec(fcs: &Fcs) -> Result<Option<SpectrumMatrixSpec>> {
    let Some((matrix, channel_refs)) = fcs
        .get_spillover_matrix()
        .context("Failed to parse $SPILLOVER/$SPILL/$COMP keyword")?
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

fn resolve_spillover_channel_names(fcs: &Fcs, refs: &[String]) -> Result<Vec<String>> {
    let n_params = *fcs
        .metadata
        .get_number_of_parameters()
        .context("Missing $PAR in FCS metadata")?;
    let mut known_channels = HashSet::<String>::with_capacity(n_params);
    for idx in 1..=n_params {
        let name = fcs
            .metadata
            .get_parameter_channel_name(idx)
            .with_context(|| format!("Missing $P{}N in FCS metadata", idx))?;
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
                .with_context(|| format!("Invalid spillover channel reference '{}'", raw))?;
            resolved.push(channel_name.to_string());
            continue;
        }
        bail!(
            "Unresolvable spillover channel reference '{}': not a channel name and not a parameter index",
            raw
        );
    }
    Ok(resolved)
}

fn collect_expected_files(expected_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(expected_dir)
        .with_context(|| format!("Failed to read {}", expected_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with("Results_") && name.ends_with(".txt") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn gate_name_from_expected_path(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Bad expected file name: {}", path.display()))?;
    let Some(gate_name) = stem.strip_prefix("Results_") else {
        bail!("Expected file must start with Results_: {}", path.display());
    };
    Ok(gate_name.to_string())
}

fn load_expected_bits(path: &Path) -> Result<Vec<bool>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut out = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let token = line.trim();
        if token.is_empty() {
            continue;
        }
        match token {
            "0" => out.push(false),
            "1" => out.push(true),
            _ => {
                bail!(
                    "Invalid expected value '{}' in {} at line {}",
                    token,
                    path.display(),
                    line_no + 1
                )
            }
        }
    }
    Ok(out)
}

fn bools_sha256(bits: &BitVec) -> String {
    let mut hasher = Sha256::new();
    for bit in bits.iter() {
        let byte = if *bit { b'1' } else { b'0' };
        hasher.update([byte]);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn gate_kind_name(gate: &GateKind) -> &'static str {
    match gate {
        GateKind::Rectangle(_) => "RectangleGate",
        GateKind::Polygon(_) => "PolygonGate",
        GateKind::Ellipsoid(_) => "EllipsoidGate",
        GateKind::Boolean(_) => "BooleanGate",
    }
}

fn dependency_ancestry(doc: &FlowGateDocument, gate_id: &GateId) -> Vec<String> {
    fn walk(
        doc: &FlowGateDocument,
        gate_id: &GateId,
        seen: &mut HashSet<String>,
        out: &mut Vec<String>,
    ) {
        let Some(gate) = doc.gate_registry.get(gate_id) else {
            return;
        };
        for dep in direct_dependencies(gate) {
            let dep_name = dep.as_str().to_string();
            if seen.insert(dep_name.clone()) {
                out.push(dep_name.clone());
                walk(doc, &dep, seen, out);
            }
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    walk(doc, gate_id, &mut seen, &mut out);
    out
}

fn direct_dependencies(gate: &GateKind) -> Vec<GateId> {
    let mut deps = Vec::new();
    if let Some(parent) = gate.parent_id() {
        deps.push(parent.clone());
    }
    if let GateKind::Boolean(boolean_gate) = gate {
        for operand in boolean_gate.operands() {
            deps.push(operand.gate_id.clone());
        }
    }
    deps
}

fn mismatch_diagnostics(
    gate: &GateKind,
    prepared: &EventMatrix,
    actual_bits: &BitVec,
    expected_bits: &[bool],
    event_index: usize,
    all_results: &HashMap<GateId, BitVec>,
) -> Option<MismatchDiagnostics> {
    let dims = gate.dimensions();
    let projection = prepared.project(dims).ok()?;
    let transforms = gate_transforms(gate);

    let mut compensated_coords = Vec::with_capacity(projection.n_cols());
    let mut transformed_coords = Vec::with_capacity(projection.n_cols());
    for (idx, col) in projection.columns().iter().enumerate() {
        let raw = col[event_index];
        let transformed = transforms
            .get(idx)
            .copied()
            .flatten()
            .map_or(raw, |t| t.apply(raw));
        compensated_coords.push(raw);
        transformed_coords.push(transformed);
    }

    let pre_parent_value = evaluate_pre_parent(gate, event_index, &transformed_coords, all_results);
    let post_parent_value = Some(actual_bits[event_index]);
    let parent_gate_id = gate.parent_id().map(|v| v.as_str().to_string());
    let parent_value = gate
        .parent_id()
        .and_then(|id| all_results.get(id))
        .map(|bits| bits[event_index]);

    Some(MismatchDiagnostics {
        event_index,
        expected_value: expected_bits[event_index],
        actual_value: actual_bits[event_index],
        pre_parent_value,
        post_parent_value,
        parent_gate_id,
        parent_value,
        compensated_coords,
        transformed_coords,
    })
}

fn evaluate_pre_parent(
    gate: &GateKind,
    event_index: usize,
    transformed_coords: &[f64],
    all_results: &HashMap<GateId, BitVec>,
) -> Option<bool> {
    match gate {
        GateKind::Rectangle(g) => Some(g.contains(transformed_coords)),
        GateKind::Polygon(g) => Some(g.contains(transformed_coords)),
        GateKind::Ellipsoid(g) => Some(g.contains(transformed_coords)),
        GateKind::Boolean(g) => evaluate_boolean_pre_parent(g, event_index, all_results),
    }
}

fn evaluate_boolean_pre_parent(
    gate: &BooleanGate,
    event_index: usize,
    all_results: &HashMap<GateId, BitVec>,
) -> Option<bool> {
    let mut operands = Vec::with_capacity(gate.operands().len());
    for operand in gate.operands() {
        let src = all_results.get(&operand.gate_id)?;
        let value = if operand.complement {
            !src[event_index]
        } else {
            src[event_index]
        };
        operands.push(value);
    }

    match gate.op() {
        BooleanOp::And => Some(operands.into_iter().all(|v| v)),
        BooleanOp::Or => Some(operands.into_iter().any(|v| v)),
        BooleanOp::Not => {
            if operands.len() != 1 {
                return None;
            }
            Some(!operands[0])
        }
    }
}

fn gate_transforms(gate: &GateKind) -> Vec<Option<TransformKind>> {
    match gate {
        GateKind::Rectangle(g) => g
            .rectangle_dimensions()
            .iter()
            .map(|d| d.transform)
            .collect(),
        GateKind::Polygon(g) => vec![g.x_dim().transform, g.y_dim().transform],
        GateKind::Ellipsoid(g) => g.dimensions_def().iter().map(|d| d.transform).collect(),
        GateKind::Boolean(_) => Vec::new(),
    }
}

fn load_summary_index(path: PathBuf) -> Result<SummaryIndex> {
    if !path.exists() {
        return Ok(SummaryIndex::default());
    }

    let mut index = SummaryIndex::default();
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_path(&path)
        .with_context(|| format!("Failed to read Summary.csv at {}", path.display()))?;
    let headers = reader.headers()?.clone();

    let idx_folder = find_header(&headers, &["Folder"]);
    let idx_gate = find_header(&headers, &["Gate"]);
    let idx_expected_file = find_header(&headers, &["Expected result details file"]);
    let idx_events_in = find_header(&headers, &["Events in"]);

    for record in reader.records() {
        let record = record?;
        let set_name = idx_folder
            .and_then(|i| record.get(i))
            .map(canonical_set_name)
            .unwrap_or_else(|| "unknown".to_string());
        let gate_name = idx_gate
            .and_then(|i| record.get(i))
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if gate_name.is_empty() {
            continue;
        }

        let expected_gate = idx_expected_file
            .and_then(|i| record.get(i))
            .and_then(gate_name_from_expected_file_name)
            .unwrap_or_else(|| gate_name.clone());

        index
            .expected_alias_by_set
            .entry(set_name.clone())
            .or_default()
            .insert(expected_gate, gate_name.clone());

        if let Some(events_col_idx) = idx_events_in {
            if let Some(raw_events) = record.get(events_col_idx) {
                if let Ok(value) = raw_events.trim().parse::<usize>() {
                    index
                        .events_in_by_set
                        .entry(set_name.clone())
                        .or_default()
                        .insert(gate_name.clone(), value);
                }
            }
        }
    }
    Ok(index)
}

fn find_header(headers: &csv::StringRecord, names: &[&str]) -> Option<usize> {
    for name in names {
        if let Some(idx) = headers.iter().position(|h| h.trim() == *name) {
            return Some(idx);
        }
    }
    None
}

fn gate_name_from_expected_file_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let leaf = Path::new(trimmed)
        .file_name()?
        .to_string_lossy()
        .to_string();
    let stem = Path::new(&leaf).file_stem()?.to_string_lossy().to_string();
    if let Some(gate_name) = stem.strip_prefix("Results_") {
        return Some(gate_name.to_string());
    }
    None
}

fn canonical_set_name(raw: &str) -> String {
    let normalized = raw.trim().to_ascii_lowercase().replace([' ', '-', '_'], "");
    if normalized == "set1" || normalized.contains("set1") {
        "set1".to_string()
    } else if normalized == "set2" || normalized.contains("set2") {
        "set2".to_string()
    } else {
        normalized
    }
}

impl GateAliasResolver {
    fn new<'a>(
        actual_gate_ids: impl Iterator<Item = &'a GateId>,
        summary_aliases: HashMap<String, String>,
    ) -> Self {
        let mut exact = HashSet::new();
        let mut lower = HashMap::<String, BTreeSet<String>>::new();
        let mut normalized = HashMap::<String, BTreeSet<String>>::new();
        let mut normalized_l1 = HashMap::<String, BTreeSet<String>>::new();

        for gate_id in actual_gate_ids {
            let gate = gate_id.as_str().to_string();
            exact.insert(gate.clone());
            lower
                .entry(gate.to_ascii_lowercase())
                .or_default()
                .insert(gate.clone());
            normalized
                .entry(normalize_gate_name(&gate))
                .or_default()
                .insert(gate.clone());
            normalized_l1
                .entry(normalize_gate_name_l1(&gate))
                .or_default()
                .insert(gate.clone());
        }

        Self {
            exact,
            lower,
            normalized,
            normalized_l1,
            summary_aliases,
        }
    }

    fn resolve(&self, expected_gate_id: &str) -> ResolvedGate {
        if self.exact.contains(expected_gate_id) {
            return ResolvedGate {
                resolved_gate_id: Some(expected_gate_id.to_string()),
                mapping_source: MappingSource::Direct,
            };
        }

        if let Some(summary_mapped) = self.summary_aliases.get(expected_gate_id) {
            if self.exact.contains(summary_mapped) {
                return ResolvedGate {
                    resolved_gate_id: Some(summary_mapped.clone()),
                    mapping_source: MappingSource::SummaryCsv,
                };
            }
        }

        let expected_lower = expected_gate_id.to_ascii_lowercase();
        if let Some(candidate) = single_candidate(self.lower.get(&expected_lower)) {
            return ResolvedGate {
                resolved_gate_id: Some(candidate),
                mapping_source: MappingSource::CaseInsensitive,
            };
        }

        let normalized = normalize_gate_name(expected_gate_id);
        if let Some(candidate) = single_candidate(self.normalized.get(&normalized)) {
            return ResolvedGate {
                resolved_gate_id: Some(candidate),
                mapping_source: MappingSource::Normalized,
            };
        }

        let normalized_l1 = normalize_gate_name_l1(expected_gate_id);
        if let Some(candidate) = single_candidate(self.normalized_l1.get(&normalized_l1)) {
            return ResolvedGate {
                resolved_gate_id: Some(candidate),
                mapping_source: MappingSource::NormalizedL1,
            };
        }

        ResolvedGate {
            resolved_gate_id: None,
            mapping_source: MappingSource::Unresolved,
        }
    }
}

fn single_candidate(candidates: Option<&BTreeSet<String>>) -> Option<String> {
    let candidates = candidates?;
    if candidates.len() == 1 {
        return candidates.iter().next().cloned();
    }
    None
}

fn normalize_gate_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn normalize_gate_name_l1(value: &str) -> String {
    normalize_gate_name(value)
        .chars()
        .map(|c| if c == 'l' || c == 'i' { '1' } else { c })
        .collect()
}

fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF8 path: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_resolver_handles_l1_normalization() {
        let ids = vec![GateId::from("Poly11".to_string())];
        let resolver = GateAliasResolver::new(ids.iter(), HashMap::new());
        let resolved = resolver.resolve("Poly1l");
        assert_eq!(resolved.resolved_gate_id, Some("Poly11".to_string()));
        assert_eq!(resolved.mapping_source.as_str(), "normalized_l1");
    }

    #[test]
    fn alias_resolver_prefers_summary_mapping() {
        let ids = vec![GateId::from("Polygon_4".to_string())];
        let mut summary = HashMap::new();
        summary.insert("Polygon4".to_string(), "Polygon_4".to_string());
        let resolver = GateAliasResolver::new(ids.iter(), summary);
        let resolved = resolver.resolve("Polygon4");
        assert_eq!(resolved.resolved_gate_id, Some("Polygon_4".to_string()));
        assert_eq!(resolved.mapping_source.as_str(), "summary_csv");
    }
}
