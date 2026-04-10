use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{bail, Context, Result};
use flow_fcs::{
    keyword::{FloatKeyword, IntegerableKeyword, MixedKeyword, StringableKeyword},
    Fcs, Keyword,
};
use flow_gate::{EventMatrix, FlowGateDocument, ParameterName, SpectrumMatrixSpec};
use serde::Serialize;

#[derive(Debug)]
struct CliArgs {
    root: PathBuf,
    set_name: String,
    n_reps: usize,
    n_warmup: usize,
    output_json: Option<PathBuf>,
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

#[derive(Serialize)]
struct NativeBenchResult {
    implementation: String,
    language: String,
    set: String,
    n_reps: usize,
    mean_ms: f64,
    sd_ms: f64,
    median_ms: f64,
    p5_ms: f64,
    p95_ms: f64,
    min_ms: f64,
    max_ms: f64,
    raw_ms: Vec<f64>,
    status: String,
}

fn main() -> Result<()> {
    let args = parse_cli_args()?;
    let set_cfg = SETS
        .iter()
        .find(|s| s.canonical_name == args.set_name.as_str())
        .copied()
        .ok_or_else(|| anyhow::anyhow!("Unknown set '{}'", args.set_name))?;
    let set_dir = resolve_set_dir(&args.root, set_cfg)?;
    let xml_path = set_dir.join(set_cfg.xml_name);
    let data_path = set_dir.join(set_cfg.data_name);

    let xml = fs::read_to_string(&xml_path)
        .with_context(|| format!("Failed to read {}", xml_path.display()))?;
    let doc = FlowGateDocument::parse_str(&xml)
        .with_context(|| format!("Failed to parse {}", xml_path.display()))?;

    let fcs = Fcs::open(path_to_str(&data_path)?)
        .with_context(|| format!("Failed to open {}", data_path.display()))?;
    let (matrix, fcs_comp) = build_scaled_event_matrix(&fcs)?;

    let prepared = doc
        .prepare_owned_matrix_with_fcs_compensation(&matrix, fcs_comp.as_ref())
        .context("Failed to prepare matrix")?;

    let total = args.n_reps + args.n_warmup;
    let mut times = Vec::with_capacity(args.n_reps);
    for i in 0..total {
        println!(
            "[bench] rust_native/{} run {}/{}",
            args.set_name,
            i + 1,
            total
        );
        let t0 = Instant::now();
        let out = doc.gate_registry.classify_all(&prepared)?;
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[bench] rust_native/{} completed in {:.4} ms",
            args.set_name, elapsed_ms
        );
        black_box(out.len());
        if i >= args.n_warmup {
            times.push(elapsed_ms);
        }
    }

    let result = summarize(args.set_name, args.n_reps, &times)?;
    if let Some(path) = args.output_json {
        let payload = serde_json::to_string_pretty(&vec![result])?;
        fs::write(&path, payload)
            .with_context(|| format!("Failed to write output JSON '{}'", path.display()))?;
    } else {
        println!("{}", serde_json::to_string_pretty(&vec![result])?);
    }

    Ok(())
}

fn summarize(set: String, n_reps: usize, times: &[f64]) -> Result<NativeBenchResult> {
    if times.is_empty() {
        bail!("No benchmark timings collected");
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let variance = sorted
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f64>()
        / sorted.len() as f64;
    let sd = variance.sqrt();
    let median = quantile(&sorted, 0.5);
    let p5 = quantile(&sorted, 0.05);
    let p95 = quantile(&sorted, 0.95);
    let min = *sorted.first().unwrap_or(&f64::NAN);
    let max = *sorted.last().unwrap_or(&f64::NAN);

    Ok(NativeBenchResult {
        implementation: "rust_native".to_string(),
        language: "Rust".to_string(),
        set,
        n_reps,
        mean_ms: mean,
        sd_ms: sd,
        median_ms: median,
        p5_ms: p5,
        p95_ms: p95,
        min_ms: min,
        max_ms: max,
        raw_ms: times.to_vec(),
        status: "ok".to_string(),
    })
}

fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return sorted[0];
    }
    if p >= 1.0 {
        return *sorted.last().unwrap_or(&sorted[0]);
    }
    let pos = (sorted.len() - 1) as f64 * p;
    let low = pos.floor() as usize;
    let high = pos.ceil() as usize;
    if low == high {
        return sorted[low];
    }
    let w = pos - low as f64;
    sorted[low] * (1.0 - w) + sorted[high] * w
}

fn parse_cli_args() -> Result<CliArgs> {
    let mut root: Option<PathBuf> = None;
    let mut set_name: Option<String> = None;
    let mut n_reps = 100usize;
    let mut n_warmup = 5usize;
    let mut output_json: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                root = args.next().map(PathBuf::from);
            }
            "--set" => {
                set_name = args.next();
            }
            "--n-reps" => {
                n_reps = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing value for --n-reps"))?
                    .parse()?;
            }
            "--n-warmup" => {
                n_warmup = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing value for --n-warmup"))?
                    .parse()?;
            }
            "--output-json" => {
                output_json = args.next().map(PathBuf::from);
            }
            other => bail!("Unknown argument '{other}'"),
        }
    }
    let root = root.ok_or_else(|| anyhow::anyhow!("Missing --root"))?;
    let set_name = set_name.ok_or_else(|| anyhow::anyhow!("Missing --set"))?;
    let normalized = canonical_set_name(&set_name);
    if normalized != "set1" && normalized != "set2" {
        bail!("--set must be set1 or set2");
    }
    Ok(CliArgs {
        root,
        set_name: normalized,
        n_reps,
        n_warmup,
        output_json,
    })
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
    let mut known_channels = std::collections::HashSet::<String>::with_capacity(n_params);
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

fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF8 path: {}", path.display()))
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
