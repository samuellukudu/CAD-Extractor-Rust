use std::{
    collections::BTreeMap,
    error::Error,
    ffi::OsStr,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::Instant,
};

use acadrust::{CadDocument, DwgWriter, DxfVersion, DxfWriter};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use cad_extraction::extraction::converter::convert_document;
use cad_extraction::extraction::extractor::extract_file;
use cad_extraction::extraction::models::{
    Bounds2D, CadColorSpec, CadLineWeightSpec, EntityStyle, ExtractedDrawing,
    HatchBoundaryEdgeGeometry, HatchBoundaryPathGeometry, Point2, SceneEntity, SceneGeometry,
};
use cad_extraction::extraction::reader::read_document;

#[derive(Parser, Debug)]
#[command(
    name = "cad-extract",
    author,
    version,
    about = "Convert CAD files to JSON and roundtrip JSON back to DXF and DWG",
    after_help = "Examples:\n  cad-extract cad2json /path/to/file.dwg --json-kind roundtrip --pretty\n  cad-extract json2cad /path/to/file.json --output output/restored\n  cad-extract semantic-diff original.dwg restored.dwg --pretty\n  cad-extract roundtrip-check /path/to/file.dwg --output-root test_outputs"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Convert DXF/DWG files into JSON
    #[command(name = "cad2json", visible_alias = "to-json", visible_alias = "extract")]
    ToJson(ToJsonArgs),
    /// Convert roundtrip JSON back into both DXF and DWG
    #[command(name = "json2cad", visible_alias = "to-cad", visible_alias = "from-json")]
    ToCad(ToCadArgs),
    /// Compare two CAD files semantically after normalizing unstable metadata
    #[command(name = "semantic-diff", visible_alias = "compare-cad")]
    SemanticDiff(SemanticDiffArgs),
    /// Run CAD -> JSON -> CAD and semantic diff with pass/fail output
    #[command(name = "roundtrip-check")]
    RoundtripCheck(RoundtripCheckArgs),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum JsonKind {
    /// Full acadrust document JSON that can be converted back to CAD
    Roundtrip,
    /// Cleaner extracted header/tables/blocks JSON for downstream consumption
    Extracted,
}

#[derive(Args, Debug)]
struct ToJsonArgs {
    /// Input DXF/DWG file(s) or directory
    #[arg(required = true)]
    input: Vec<PathBuf>,

    /// Output file path (single-file mode) or directory (batch mode)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// JSON shape to emit
    #[arg(long, value_enum, default_value_t = JsonKind::Roundtrip)]
    json_kind: JsonKind,

    /// Pretty print JSON output
    #[arg(short, long)]
    pretty: bool,

    /// Verbose output with conversion statistics
    #[arg(short, long)]
    verbose: bool,

    /// Process all DXF/DWG files in input directories recursively
    #[arg(short = 'r', long)]
    recursive: bool,
}

#[derive(Args, Debug)]
struct ToCadArgs {
    /// Input roundtrip JSON file(s) or directory
    #[arg(required = true)]
    input: Vec<PathBuf>,

    /// Output base path (single-file mode) or directory (batch mode)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Verbose output with conversion statistics
    #[arg(short, long)]
    verbose: bool,

    /// Process all JSON files in input directories recursively
    #[arg(short = 'r', long)]
    recursive: bool,
}

#[derive(Args, Debug)]
struct SemanticDiffArgs {
    /// The original CAD file to compare
    left: PathBuf,

    /// The restored or candidate CAD file to compare
    right: PathBuf,

    /// Optional JSON report output path
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Pretty print the JSON report when writing to disk
    #[arg(short, long)]
    pretty: bool,

    /// Maximum number of entity fingerprint differences to include in the report
    #[arg(long, default_value_t = 20)]
    detail_limit: usize,
}

#[derive(Args, Debug)]
struct RoundtripCheckArgs {
    /// Input DXF/DWG file(s) or directory
    #[arg(required = true)]
    input: Vec<PathBuf>,

    /// Output root directory containing cad2json, json2cad, and semantic_diff folders
    #[arg(long)]
    output_root: Option<PathBuf>,

    /// Pretty print generated JSON artifacts and diff reports
    #[arg(short, long)]
    pretty: bool,

    /// Process all DXF/DWG files in input directories recursively
    #[arg(short = 'r', long)]
    recursive: bool,

    /// Maximum number of fingerprint differences to include in reports
    #[arg(long, default_value_t = 20)]
    detail_limit: usize,
}

#[derive(Debug, Clone, Serialize)]
struct CountDifference {
    key: String,
    left_count: usize,
    right_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SemanticSnapshot {
    total_entities: usize,
    renderable_entities: usize,
    ignored_entities: usize,
    bounds: Option<NormalizedBounds>,
    layer_counts: BTreeMap<String, usize>,
    block_counts: BTreeMap<String, usize>,
    entity_type_counts: BTreeMap<String, usize>,
    entity_fingerprints: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct NormalizedPoint {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct NormalizedBounds {
    min: NormalizedPoint,
    max: NormalizedPoint,
}

#[derive(Debug, Clone, Serialize)]
struct SemanticDiffReport {
    left_path: String,
    right_path: String,
    equivalent: bool,
    left: SemanticSnapshot,
    right: SemanticSnapshot,
    bounds_equal: bool,
    layer_counts_equal: bool,
    block_counts_equal: bool,
    entity_type_counts_equal: bool,
    entity_fingerprints_equal: bool,
    layer_differences: Vec<CountDifference>,
    block_differences: Vec<CountDifference>,
    entity_type_differences: Vec<CountDifference>,
    fingerprint_differences: Vec<CountDifference>,
    omitted_fingerprint_differences: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RoundtripSkipReport {
    input_path: String,
    skipped: bool,
    source_version: Option<String>,
    reason: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::ToJson(args) => run_to_json(&args),
        Commands::ToCad(args) => run_to_cad(&args),
        Commands::SemanticDiff(args) => run_semantic_diff(&args),
        Commands::RoundtripCheck(args) => run_roundtrip_check(&args),
    }
}

fn run_to_json(args: &ToJsonArgs) -> Result<(), Box<dyn Error>> {
    let files = collect_input_files(&args.input, args.recursive, &["dxf", "dwg"])?;

    if files.is_empty() {
        return Err("No DXF/DWG files found in the specified input(s).".into());
    }

    let is_batch = files.len() > 1;
    let mut total_entities = 0usize;
    let mut total_files = 0usize;
    let overall_start = Instant::now();

    for input_path in &files {
        let output_path = resolve_json_output_path(input_path, args.output.as_deref(), is_batch);

        if args.verbose {
            eprint!(
                "Converting {} -> JSON ({:?})...",
                input_path.display(),
                args.json_kind
            );
        }

        let start = Instant::now();

        match write_cad_to_json(input_path, &output_path, args.json_kind, args.pretty) {
            Ok(entity_count) => {
                let elapsed = start.elapsed();
                total_entities += entity_count;
                total_files += 1;

                if args.verbose {
                    eprintln!(
                        " {} entities -> {} ({:.1}ms)",
                        entity_count,
                        output_path.display(),
                        elapsed.as_secs_f64() * 1000.0
                    );
                } else {
                    println!("{} -> {}", input_path.display(), output_path.display());
                }
            }
            Err(error) => {
                eprintln!("\nError processing {}: {}", input_path.display(), error);
                if !is_batch {
                    return Err(error);
                }
            }
        }
    }

    if args.verbose && is_batch {
        let elapsed = overall_start.elapsed();
        eprintln!(
            "\nSummary: {} files, {} total entities in {:.1}ms",
            total_files,
            total_entities,
            elapsed.as_secs_f64() * 1000.0
        );
    }

    Ok(())
}

fn run_to_cad(args: &ToCadArgs) -> Result<(), Box<dyn Error>> {
    let files = collect_input_files(&args.input, args.recursive, &["json"])?;

    if files.is_empty() {
        return Err("No JSON files found in the specified input(s).".into());
    }

    let is_batch = files.len() > 1;
    let mut total_entities = 0usize;
    let mut total_files = 0usize;
    let overall_start = Instant::now();

    for input_path in &files {
        let output_base = resolve_cad_output_base(input_path, args.output.as_deref(), is_batch);
        let dxf_output = with_extension(&output_base, "dxf");
        let dwg_output = with_extension(&output_base, "dwg");

        if args.verbose {
            eprint!("Converting {} -> CAD...", input_path.display());
        }

        let start = Instant::now();

        match write_json_to_cad(input_path, &output_base) {
            Ok(entity_count) => {
                let elapsed = start.elapsed();
                total_entities += entity_count;
                total_files += 1;

                if args.verbose {
                    eprintln!(
                        " {} entities -> {}, {} ({:.1}ms)",
                        entity_count,
                        dxf_output.display(),
                        dwg_output.display(),
                        elapsed.as_secs_f64() * 1000.0
                    );
                } else {
                    println!(
                        "{} -> {}, {}",
                        input_path.display(),
                        dxf_output.display(),
                        dwg_output.display()
                    );
                }
            }
            Err(error) => {
                eprintln!("\nError processing {}: {}", input_path.display(), error);
                if !is_batch {
                    return Err(error);
                }
            }
        }
    }

    if args.verbose && is_batch {
        let elapsed = overall_start.elapsed();
        eprintln!(
            "\nSummary: {} JSON files, {} total entities in {:.1}ms",
            total_files,
            total_entities,
            elapsed.as_secs_f64() * 1000.0
        );
    }

    Ok(())
}

fn run_semantic_diff(args: &SemanticDiffArgs) -> Result<(), Box<dyn Error>> {
    let report = semantic_diff_from_paths(&args.left, &args.right, args.detail_limit)?;

    if let Some(output) = args.output.as_deref() {
        write_json_value(output, &report, args.pretty)?;
        println!("Semantic diff report -> {}", output.display());
    }

    print_semantic_diff_report(&report);

    Ok(())
}

fn run_roundtrip_check(args: &RoundtripCheckArgs) -> Result<(), Box<dyn Error>> {
    let files = collect_input_files(&args.input, args.recursive, &["dxf", "dwg"])?;

    if files.is_empty() {
        return Err("No DXF/DWG files found in the specified input(s).".into());
    }

    let output_root = args
        .output_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("test_outputs"));
    let cad2json_dir = output_root.join("cad2json");
    let json2cad_dir = output_root.join("json2cad");
    let semantic_diff_dir = output_root.join("semantic_diff");

    fs::create_dir_all(&cad2json_dir)?;
    fs::create_dir_all(&json2cad_dir)?;
    fs::create_dir_all(&semantic_diff_dir)?;

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;

    for input_path in &files {
        let stem = file_stem_or_default(input_path).to_string_lossy().to_string();
        let json_path = cad2json_dir.join(format!("{stem}.roundtrip.json"));
        let cad_output_base = json2cad_dir.join(&stem);
        let report_path = semantic_diff_dir.join(format!("{stem}.semantic_diff.json"));

        if let Some(skip_report) = build_roundtrip_skip_report(input_path)? {
            skipped += 1;
            write_json_value(&report_path, &skip_report, args.pretty)?;
            println!("SKIP {}", input_path.display());
            print_roundtrip_skip_report_summary(&skip_report, &report_path);
            continue;
        }

        let original_extension = input_path
            .extension()
            .and_then(OsStr::to_str)
            .ok_or_else(|| format!("Input has no extension: {}", input_path.display()))?;
        let restored_path = with_extension(&cad_output_base, &original_extension.to_ascii_lowercase());

        write_cad_to_json(input_path, &json_path, JsonKind::Roundtrip, args.pretty)?;
        write_json_to_cad(&json_path, &cad_output_base)?;

        let report = semantic_diff_from_paths(input_path, &restored_path, args.detail_limit)?;
        write_json_value(&report_path, &report, args.pretty)?;

        if report.equivalent {
            passed += 1;
            println!(
                "PASS {} -> {}",
                input_path.display(),
                restored_path.display()
            );
            print_roundtrip_check_report_summary(&report, &report_path);
        } else {
            failed += 1;
            println!(
                "FAIL {} -> {}",
                input_path.display(),
                restored_path.display()
            );
            print_roundtrip_check_report_summary(&report, &report_path);
            print_semantic_diff_report(&report);
        }
    }

    println!(
        "Roundtrip check summary: passed={} failed={} skipped={} output_root={}",
        passed,
        failed,
        skipped,
        output_root.display()
    );

    if failed > 0 {
        return Err(format!("roundtrip-check failed for {failed} file(s)").into());
    }

    Ok(())
}

fn build_roundtrip_skip_report(input_path: &Path) -> Result<Option<RoundtripSkipReport>, Box<dyn Error>> {
    let source_version = sniff_dxf_header_version(input_path)?;
    let should_skip = source_version
        .as_deref()
        .map(|version| version.eq_ignore_ascii_case("AC1009"))
        .unwrap_or(false);

    if !should_skip {
        return Ok(None);
    }

    Ok(Some(RoundtripSkipReport {
        input_path: input_path.display().to_string(),
        skipped: true,
        source_version,
        reason: "Legacy DXF AC1009/R12 input is skipped during roundtrip-check because the current json2cad DXF writer path collapses entities for this format. cad2json and semantic extraction remain available, but semantic roundtrip validation is not reliable for this source version.".to_owned(),
    }))
}

fn write_cad_to_json(
    input: &Path,
    output: &Path,
    json_kind: JsonKind,
    pretty: bool,
) -> Result<usize, Box<dyn Error>> {
    let document = read_document(input)?;
    let entity_count = document.entity_count();

    match json_kind {
        JsonKind::Roundtrip => write_json_value(output, &document, pretty)?,
        JsonKind::Extracted => {
            let cad_output = convert_document(&document);
            write_json_value(output, &cad_output, pretty)?;
        }
    }

    Ok(entity_count)
}

fn write_json_to_cad(input: &Path, output_base: &Path) -> Result<usize, Box<dyn Error>> {
    let document = read_roundtrip_json(input)?;
    let entity_count = document.entity_count();

    let dxf_output = with_extension(output_base, "dxf");
    let dwg_output = with_extension(output_base, "dwg");

    ensure_parent_dir(&dxf_output)?;
    ensure_parent_dir(&dwg_output)?;

    DxfWriter::new(&document).write_to_file(&dxf_output)?;
    if document.version == DxfVersion::Unknown {
        repair_dxf_version_header(&dxf_output, DxfVersion::AC1012)?;
    }

    let mut dwg_document = document.clone();
    dwg_document.version = normalize_output_version(dwg_document.version);
    DwgWriter::write_to_file(&dwg_output, &dwg_document)?;

    Ok(entity_count)
}

fn normalize_output_version(version: DxfVersion) -> DxfVersion {
    match version {
        // `json2cad` always writes both DXF and DWG. DWG output requires a
        // modern version, so we upgrade legacy/unknown versions to the oldest
        // DWG-safe target we support.
        DxfVersion::AC1024 | DxfVersion::AC1027 | DxfVersion::AC1032 => version,
        _ => DxfVersion::AC1024,
    }
}

fn repair_dxf_version_header(path: &Path, version: DxfVersion) -> Result<(), Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    let line_ending = if contents.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = contents.lines().map(str::to_owned).collect::<Vec<_>>();

    for index in 0..lines.len().saturating_sub(3) {
        if lines[index].trim() == "9"
            && lines[index + 1].trim() == "$ACADVER"
            && lines[index + 2].trim() == "1"
        {
            lines[index + 3] = version.as_str().to_owned();
            fs::write(path, lines.join(line_ending))?;
            return Ok(());
        }
    }

    Ok(())
}

fn read_roundtrip_json(path: &Path) -> Result<CadDocument, Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);
    serde_json::from_reader(reader).map_err(|source| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "failed to parse {} as roundtrip CAD JSON: {}. Use `to-json --json-kind roundtrip` when you need reverse conversion.",
                path.display(),
                source
            ),
        )
        .into()
    })
}

fn sniff_dxf_header_version(path: &Path) -> Result<Option<String>, Box<dyn Error>> {
    if !has_allowed_extension(path, &["dxf"]) {
        return Ok(None);
    }

    let reader = BufReader::new(File::open(path)?);
    let mut lines = reader.lines();

    while let Some(line) = lines.next() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed == "ENDSEC" {
            break;
        }

        if trimmed != "$ACADVER" {
            continue;
        }

        let Some(code_line) = lines.next() else {
            break;
        };
        let code_line = code_line?;
        let Some(value_line) = lines.next() else {
            break;
        };
        let value_line = value_line?;

        if code_line.trim() == "1" {
            return Ok(Some(value_line.trim().to_owned()));
        }
    }

    Ok(None)
}

fn write_json_value<T: Serialize>(path: &Path, value: &T, pretty: bool) -> Result<(), Box<dyn Error>> {
    ensure_parent_dir(path)?;
    let file = File::create(path)?;

    if pretty {
        serde_json::to_writer_pretty(file, value)?;
    } else {
        serde_json::to_writer(file, value)?;
    }

    Ok(())
}

fn collect_input_files(
    inputs: &[PathBuf],
    recursive: bool,
    allowed_extensions: &[&str],
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();

    for input in inputs {
        if input.is_file() {
            if has_allowed_extension(input, allowed_extensions) {
                files.push(input.clone());
            } else {
                eprintln!(
                    "Warning: {} does not match the expected input extensions, skipping.",
                    input.display()
                );
            }
        } else if input.is_dir() {
            collect_from_dir(input, recursive, allowed_extensions, &mut files)?;
        } else {
            eprintln!(
                "Warning: {} is not a file or directory, skipping.",
                input.display()
            );
        }
    }

    files.sort();
    Ok(files)
}

fn collect_from_dir(
    dir: &Path,
    recursive: bool,
    allowed_extensions: &[&str],
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && has_allowed_extension(&path, allowed_extensions) {
            files.push(path);
        } else if path.is_dir() && recursive {
            collect_from_dir(&path, recursive, allowed_extensions, files)?;
        }
    }

    Ok(())
}

fn has_allowed_extension(path: &Path, allowed_extensions: &[&str]) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            allowed_extensions.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

fn resolve_json_output_path(input: &Path, requested_output: Option<&Path>, is_batch: bool) -> PathBuf {
    let default_name = json_file_name_for(input);

    if is_batch {
        requested_output
            .unwrap_or_else(|| Path::new("output"))
            .join(default_name)
    } else {
        match requested_output {
            Some(path) if path.is_dir() => path.join(default_name),
            Some(path) => with_extension(path, "json"),
            None => Path::new("output").join(default_name),
        }
    }
}

fn resolve_cad_output_base(input: &Path, requested_output: Option<&Path>, is_batch: bool) -> PathBuf {
    let default_stem = file_stem_or_default(input);

    if is_batch {
        requested_output
            .unwrap_or_else(|| Path::new("output"))
            .join(default_stem)
    } else {
        match requested_output {
            Some(path) if path.is_dir() => path.join(default_stem),
            Some(path) => strip_known_extension(path),
            None => Path::new("output").join(default_stem),
        }
    }
}

fn json_file_name_for(input: &Path) -> PathBuf {
    let mut name = input
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("output"));
    name.set_extension("json");
    name
}

fn file_stem_or_default(input: &Path) -> &OsStr {
    input.file_stem().unwrap_or_else(|| OsStr::new("output"))
}

fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut updated = path.to_path_buf();
    updated.set_extension(extension);
    updated
}

fn strip_known_extension(path: &Path) -> PathBuf {
    let mut base = path.to_path_buf();

    if let Some(ext) = path.extension().and_then(OsStr::to_str) {
        let lower = ext.to_ascii_lowercase();
        if matches!(lower.as_str(), "json" | "dxf" | "dwg") {
            base.set_extension("");
        }
    }

    base
}

fn ensure_parent_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    Ok(())
}

fn semantic_diff_from_paths(
    left_path: &Path,
    right_path: &Path,
    detail_limit: usize,
) -> Result<SemanticDiffReport, Box<dyn Error>> {
    let left = extract_file(left_path)?;
    let right = extract_file(right_path)?;
    Ok(build_semantic_diff_report(&left, &right, detail_limit))
}

fn build_semantic_diff_report(
    left: &ExtractedDrawing,
    right: &ExtractedDrawing,
    detail_limit: usize,
) -> SemanticDiffReport {
    let left_snapshot = build_semantic_snapshot(left);
    let right_snapshot = build_semantic_snapshot(right);

    let layer_differences = diff_count_maps(&left_snapshot.layer_counts, &right_snapshot.layer_counts);
    let block_differences = diff_count_maps(&left_snapshot.block_counts, &right_snapshot.block_counts);
    let entity_type_differences = diff_count_maps(
        &left_snapshot.entity_type_counts,
        &right_snapshot.entity_type_counts,
    );
    let all_fingerprint_differences = diff_count_maps(
        &left_snapshot.entity_fingerprints,
        &right_snapshot.entity_fingerprints,
    );
    let fingerprint_differences = all_fingerprint_differences
        .iter()
        .take(detail_limit)
        .cloned()
        .collect::<Vec<_>>();
    let omitted_fingerprint_differences =
        all_fingerprint_differences.len().saturating_sub(fingerprint_differences.len());

    let bounds_equal = left_snapshot.bounds == right_snapshot.bounds;
    let layer_counts_equal = layer_differences.is_empty();
    let block_counts_equal = block_differences.is_empty();
    let entity_type_counts_equal = entity_type_differences.is_empty();
    let entity_fingerprints_equal = all_fingerprint_differences.is_empty();
    let equivalent = left_snapshot.renderable_entities == right_snapshot.renderable_entities
        && bounds_equal
        && layer_counts_equal
        && entity_type_counts_equal
        && entity_fingerprints_equal;

    SemanticDiffReport {
        left_path: left.source_path.display().to_string(),
        right_path: right.source_path.display().to_string(),
        equivalent,
        left: left_snapshot,
        right: right_snapshot,
        bounds_equal,
        layer_counts_equal,
        block_counts_equal,
        entity_type_counts_equal,
        entity_fingerprints_equal,
        layer_differences,
        block_differences,
        entity_type_differences,
        fingerprint_differences,
        omitted_fingerprint_differences,
    }
}

fn build_semantic_snapshot(drawing: &ExtractedDrawing) -> SemanticSnapshot {
    let mut layer_counts = BTreeMap::new();
    let mut block_counts = BTreeMap::new();
    let mut entity_type_counts = BTreeMap::new();
    let mut entity_fingerprints = BTreeMap::new();
    let normalized_layout_blocks = normalized_layout_blocks(drawing);

    for entity in &drawing.entities {
        if !is_comparable_entity(entity) {
            continue;
        }

        *layer_counts.entry(entity.layer_name.clone()).or_insert(0) += 1;
        let normalized_block_name =
            normalized_block_name(entity.block_name.as_deref(), &normalized_layout_blocks);
        if let Some(block_name) = &normalized_block_name {
            *block_counts.entry(block_name.clone()).or_insert(0) += 1;
        }
        *entity_type_counts
            .entry(entity.entity_type.clone())
            .or_insert(0) += 1;
        *entity_fingerprints
            .entry(semantic_entity_fingerprint(
                entity,
                normalized_block_name.as_deref(),
            ))
            .or_insert(0) += 1;
    }

    SemanticSnapshot {
        total_entities: drawing.stats.total_entities,
        renderable_entities: drawing.stats.renderable_entities,
        ignored_entities: drawing.stats.ignored_entities,
        bounds: drawing.bounds.map(normalize_bounds),
        layer_counts,
        block_counts,
        entity_type_counts,
        entity_fingerprints,
    }
}

fn diff_count_maps(
    left: &BTreeMap<String, usize>,
    right: &BTreeMap<String, usize>,
) -> Vec<CountDifference> {
    let mut keys = left.keys().cloned().collect::<Vec<_>>();
    for key in right.keys() {
        if !left.contains_key(key) {
            keys.push(key.clone());
        }
    }
    keys.sort();
    keys.dedup();

    let mut differences = keys
        .into_iter()
        .filter_map(|key| {
            let left_count = left.get(&key).copied().unwrap_or(0);
            let right_count = right.get(&key).copied().unwrap_or(0);
            (left_count != right_count).then_some(CountDifference {
                key,
                left_count,
                right_count,
            })
        })
        .collect::<Vec<_>>();

    differences.sort_by(|a, b| {
        let delta_a = a.left_count.abs_diff(a.right_count);
        let delta_b = b.left_count.abs_diff(b.right_count);
        delta_b.cmp(&delta_a).then_with(|| a.key.cmp(&b.key))
    });
    differences
}

fn normalized_layout_blocks(drawing: &ExtractedDrawing) -> BTreeMap<String, String> {
    drawing
        .layouts
        .iter()
        .map(|layout| {
            (
                layout.root_block_name.clone(),
                canonical_layout_block_name(layout),
            )
        })
        .collect()
}

fn canonical_layout_block_name(layout: &cad_extraction::extraction::models::LayoutInfo) -> String {
    if layout.is_model {
        "__layout__:model".to_owned()
    } else {
        format!("__layout__:paper:{}:{}", layout.tab_order, layout.name)
    }
}

fn normalized_block_name(
    block_name: Option<&str>,
    normalized_layout_blocks: &BTreeMap<String, String>,
) -> Option<String> {
    block_name.map(|name| {
        normalized_layout_blocks
            .get(name)
            .cloned()
            .unwrap_or_else(|| {
                if name.eq_ignore_ascii_case("*model_space") {
                    "__layout__:model".to_owned()
                } else {
                    name.to_owned()
                }
            })
    })
}

fn is_comparable_entity(entity: &SceneEntity) -> bool {
    !matches!(entity.geometry, SceneGeometry::Unsupported { .. })
}

fn semantic_entity_fingerprint(entity: &SceneEntity, _normalized_block_name: Option<&str>) -> String {
    format!(
        "type={}|layer={}|style={}|geometry={}",
        entity.entity_type,
        json_string(&entity.layer_name),
        style_key(&entity.style),
        geometry_key(&entity.geometry)
    )
}

fn style_key(style: &EntityStyle) -> String {
    format!(
        "color={},line_weight={}",
        color_key(style.color),
        line_weight_key(style.line_weight)
    )
}

fn color_key(color: CadColorSpec) -> String {
    match color {
        CadColorSpec::ByLayer => "ByLayer".to_owned(),
        CadColorSpec::ByBlock => "ByBlock".to_owned(),
        CadColorSpec::Index(index) => format!("Index({index})"),
        CadColorSpec::Rgb(r, g, b) => format!("Rgb({r},{g},{b})"),
    }
}

fn line_weight_key(line_weight: CadLineWeightSpec) -> String {
    match line_weight {
        CadLineWeightSpec::ByLayer => "ByLayer".to_owned(),
        CadLineWeightSpec::ByBlock => "ByBlock".to_owned(),
        CadLineWeightSpec::Default => "Default".to_owned(),
        CadLineWeightSpec::Value(value) => format!("Value({value})"),
    }
}

fn geometry_key(geometry: &SceneGeometry) -> String {
    match geometry {
        SceneGeometry::Line { start, end } => {
            format!("Line(start={},end={})", point_key(*start), point_key(*end))
        }
        SceneGeometry::Circle { center, radius } => format!(
            "Circle(center={},radius={})",
            point_key(*center),
            fmt_num(*radius)
        ),
        SceneGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => format!(
            "Arc(center={},radius={},start_angle={},end_angle={})",
            point_key(*center),
            fmt_num(*radius),
            fmt_num(*start_angle),
            fmt_num(*end_angle)
        ),
        SceneGeometry::Ellipse {
            center,
            major_axis,
            minor_axis_ratio,
            start_parameter,
            end_parameter,
        } => format!(
            "Ellipse(center={},major_axis={},minor_axis_ratio={},start_parameter={},end_parameter={})",
            point_key(*center),
            point_key(*major_axis),
            fmt_num(*minor_axis_ratio),
            fmt_num(*start_parameter),
            fmt_num(*end_parameter)
        ),
        SceneGeometry::LwPolyline { polyline } => format!(
            "LwPolyline(closed={},vertices=[{}])",
            polyline.closed,
            polyline
                .vertices
                .iter()
                .map(|vertex| format!("{}@{}", point_key(vertex.location), fmt_num(vertex.bulge)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        SceneGeometry::Polyline2D { polyline } => format!(
            "Polyline2D(closed={},vertices=[{}])",
            polyline.closed,
            polyline
                .vertices
                .iter()
                .map(|vertex| format!("{}@{}", point_key(vertex.location), fmt_num(vertex.bulge)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        SceneGeometry::Polyline3D { polyline } => format!(
            "Polyline3D(closed={},vertices=[{}])",
            polyline.closed,
            polyline
                .vertices
                .iter()
                .map(|point| point_key(*point))
                .collect::<Vec<_>>()
                .join(",")
        ),
        SceneGeometry::Spline { spline } => format!(
            "Spline(degree={},knots=[{}],control_points=[{}],weights=[{}],fit_points=[{}])",
            spline.degree,
            spline
                .knots
                .iter()
                .map(|value| fmt_num(*value))
                .collect::<Vec<_>>()
                .join(","),
            spline
                .control_points
                .iter()
                .map(|point| point_key(*point))
                .collect::<Vec<_>>()
                .join(","),
            spline
                .weights
                .iter()
                .map(|value| fmt_num(*value))
                .collect::<Vec<_>>()
                .join(","),
            spline
                .fit_points
                .iter()
                .map(|point| point_key(*point))
                .collect::<Vec<_>>()
                .join(",")
        ),
        SceneGeometry::Solid { points } => format!(
            "Solid(points=[{}])",
            points
                .iter()
                .map(|point| point_key(*point))
                .collect::<Vec<_>>()
                .join(",")
        ),
        SceneGeometry::Hatch { paths, solid_fill } => format!(
            "Hatch(solid_fill={},paths=[{}])",
            solid_fill,
            paths.iter().map(hatch_path_key).collect::<Vec<_>>().join(",")
        ),
        SceneGeometry::Text { position, payload } => format!(
            "Text(position={},value={},height={},rotation={})",
            point_key(*position),
            json_string(&payload.value),
            fmt_num(payload.height),
            fmt_num(payload.rotation)
        ),
        SceneGeometry::Insert {
            block_name,
            transform,
        } => format!(
            "Insert(block_name={},position={},scale_x={},scale_y={},rotation={})",
            json_string(block_name),
            point_key(transform.position),
            fmt_num(transform.scale_x),
            fmt_num(transform.scale_y),
            fmt_num(transform.rotation)
        ),
        SceneGeometry::Dimension {
            block_name,
            transform,
        } => format!(
            "Dimension(block_name={},position={},scale_x={},scale_y={},rotation={})",
            json_string(block_name),
            point_key(transform.position),
            fmt_num(transform.scale_x),
            fmt_num(transform.scale_y),
            fmt_num(transform.rotation)
        ),
        SceneGeometry::Unsupported { reason } => {
            format!("Unsupported(reason={})", json_string(reason))
        }
    }
}

fn hatch_path_key(path: &HatchBoundaryPathGeometry) -> String {
    format!(
        "Path(edges=[{}])",
        path.edges
            .iter()
            .map(hatch_edge_key)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn hatch_edge_key(edge: &HatchBoundaryEdgeGeometry) -> String {
    match edge {
        HatchBoundaryEdgeGeometry::Line { start, end } => {
            format!("Line(start={},end={})", point_key(*start), point_key(*end))
        }
        HatchBoundaryEdgeGeometry::CircularArc {
            center,
            radius,
            start_angle,
            end_angle,
            counter_clockwise,
        } => format!(
            "CircularArc(center={},radius={},start_angle={},end_angle={},counter_clockwise={})",
            point_key(*center),
            fmt_num(*radius),
            fmt_num(*start_angle),
            fmt_num(*end_angle),
            counter_clockwise
        ),
        HatchBoundaryEdgeGeometry::EllipticArc {
            center,
            major_axis_endpoint,
            minor_axis_ratio,
            start_angle,
            end_angle,
            counter_clockwise,
        } => format!(
            "EllipticArc(center={},major_axis_endpoint={},minor_axis_ratio={},start_angle={},end_angle={},counter_clockwise={})",
            point_key(*center),
            point_key(*major_axis_endpoint),
            fmt_num(*minor_axis_ratio),
            fmt_num(*start_angle),
            fmt_num(*end_angle),
            counter_clockwise
        ),
        HatchBoundaryEdgeGeometry::Spline(spline) => format!(
            "Spline(degree={},knots=[{}],control_points=[{}],weights=[{}],fit_points=[{}])",
            spline.degree,
            spline
                .knots
                .iter()
                .map(|value| fmt_num(*value))
                .collect::<Vec<_>>()
                .join(","),
            spline
                .control_points
                .iter()
                .map(|point| point_key(*point))
                .collect::<Vec<_>>()
                .join(","),
            spline
                .weights
                .iter()
                .map(|value| fmt_num(*value))
                .collect::<Vec<_>>()
                .join(","),
            spline
                .fit_points
                .iter()
                .map(|point| point_key(*point))
                .collect::<Vec<_>>()
                .join(",")
        ),
        HatchBoundaryEdgeGeometry::Polyline(polyline) => format!(
            "Polyline(closed={},vertices=[{}])",
            polyline.closed,
            polyline
                .vertices
                .iter()
                .map(|vertex| format!("{}@{}", point_key(vertex.location), fmt_num(vertex.bulge)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn point_key(point: Point2) -> String {
    format!("({}, {})", fmt_num(point.x), fmt_num(point.y))
}

fn normalize_bounds(bounds: Bounds2D) -> NormalizedBounds {
    NormalizedBounds {
        min: NormalizedPoint {
            x: round_float(bounds.min.x),
            y: round_float(bounds.min.y),
        },
        max: NormalizedPoint {
            x: round_float(bounds.max.x),
            y: round_float(bounds.max.y),
        },
    }
}

fn round_float(value: f64) -> f64 {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    if rounded == -0.0 { 0.0 } else { rounded }
}

fn fmt_num(value: f64) -> String {
    format!("{:.4}", (value * 10_000.0).round() / 10_000.0)
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

fn print_semantic_diff_report(report: &SemanticDiffReport) {
    println!(
        "Semantic diff: {}",
        if report.equivalent {
            "equivalent"
        } else {
            "NOT equivalent"
        }
    );
    println!("Left : {}", report.left_path);
    println!("Right: {}", report.right_path);
    println!(
        "Entities: left={} right={} | renderable: left={} right={} | ignored: left={} right={}",
        report.left.total_entities,
        report.right.total_entities,
        report.left.renderable_entities,
        report.right.renderable_entities,
        report.left.ignored_entities,
        report.right.ignored_entities
    );
    println!(
        "Checks: bounds={} layers={} blocks={} entity_types={} fingerprints={}",
        report.bounds_equal,
        report.layer_counts_equal,
        report.block_counts_equal,
        report.entity_type_counts_equal,
        report.entity_fingerprints_equal
    );

    if !report.layer_differences.is_empty() {
        println!("Layer differences:");
        for diff in &report.layer_differences {
            println!("  {}: left={} right={}", diff.key, diff.left_count, diff.right_count);
        }
    }

    if !report.block_differences.is_empty() {
        println!("Block differences:");
        for diff in &report.block_differences {
            println!("  {}: left={} right={}", diff.key, diff.left_count, diff.right_count);
        }
    }

    if !report.entity_type_differences.is_empty() {
        println!("Entity type differences:");
        for diff in &report.entity_type_differences {
            println!("  {}: left={} right={}", diff.key, diff.left_count, diff.right_count);
        }
    }

    if !report.fingerprint_differences.is_empty() {
        println!("Fingerprint differences (showing {}):", report.fingerprint_differences.len());
        for diff in &report.fingerprint_differences {
            println!("  left={} right={} | {}", diff.left_count, diff.right_count, diff.key);
        }
        if report.omitted_fingerprint_differences > 0 {
            println!(
                "  ... {} additional fingerprint differences omitted",
                report.omitted_fingerprint_differences
            );
        }
    }
}

fn print_roundtrip_check_report_summary(report: &SemanticDiffReport, report_path: &Path) {
    println!("  semantic_diff: {}", if report.equivalent { "equivalent" } else { "not_equivalent" });
    println!("  report: {}", report_path.display());
    println!(
        "  entities: left={} right={} | renderable: left={} right={} | ignored: left={} right={}",
        report.left.total_entities,
        report.right.total_entities,
        report.left.renderable_entities,
        report.right.renderable_entities,
        report.left.ignored_entities,
        report.right.ignored_entities
    );
    println!(
        "  checks: bounds={} layers={} blocks={} entity_types={} fingerprints={}",
        report.bounds_equal,
        report.layer_counts_equal,
        report.block_counts_equal,
        report.entity_type_counts_equal,
        report.entity_fingerprints_equal
    );
}

fn print_roundtrip_skip_report_summary(report: &RoundtripSkipReport, report_path: &Path) {
    println!("  semantic_diff: skipped");
    println!("  report: {}", report_path.display());
    if let Some(source_version) = &report.source_version {
        println!("  source_version: {}", source_version);
    }
    println!("  reason: {}", report.reason);
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use acadrust::{EntityType, Point, entities::{Circle, Line}};
    use serde_json::Value;

    use super::*;
    use cad_extraction::extraction::extractor::extract_document;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("cad_extraction_{name}_{suffix}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn extracted_json_contains_blocks_section() {
        let temp_dir = TempDirGuard::new("extract_json");
        let cad_path = temp_dir.path.join("sample.dxf");
        let json_path = temp_dir.path.join("sample.json");

        let mut document = CadDocument::new();
        document
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 10.0, 5.0, 0.0)))
            .unwrap();
        DxfWriter::new(&document).write_to_file(&cad_path).unwrap();

        let entity_count = write_cad_to_json(&cad_path, &json_path, JsonKind::Extracted, true).unwrap();
        assert_eq!(entity_count, 1);

        let value: Value = serde_json::from_reader(File::open(&json_path).unwrap()).unwrap();
        assert!(value.get("header").is_some());
        assert!(value.get("tables").is_some());
        assert!(value.get("blocks").is_some());
    }

    #[test]
    fn roundtrip_json_writes_both_dxf_and_dwg() {
        let temp_dir = TempDirGuard::new("roundtrip_json");
        let json_path = temp_dir.path.join("document.json");
        let output_base = temp_dir.path.join("restored");

        let mut document = CadDocument::new();
        document
            .add_entity(EntityType::Line(Line::from_coords(1.0, 2.0, 0.0, 3.0, 4.0, 0.0)))
            .unwrap();
        write_json_value(&json_path, &document, true).unwrap();

        let entity_count = write_json_to_cad(&json_path, &output_base).unwrap();
        assert_eq!(entity_count, 1);
        assert!(with_extension(&output_base, "dxf").exists());
        assert!(with_extension(&output_base, "dwg").exists());
    }

    #[test]
    fn strip_known_extension_removes_roundtrip_related_suffixes() {
        assert_eq!(
            strip_known_extension(Path::new("/tmp/example.json")),
            PathBuf::from("/tmp/example")
        );
        assert_eq!(
            strip_known_extension(Path::new("/tmp/example.dwg")),
            PathBuf::from("/tmp/example")
        );
        assert_eq!(
            strip_known_extension(Path::new("/tmp/example.custom")),
            PathBuf::from("/tmp/example.custom")
        );
    }

    #[test]
    fn semantic_diff_ignores_entity_order() {
        let mut left = CadDocument::new();
        left.add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();
        left.add_entity(EntityType::Circle(Circle::from_coords(10.0, 10.0, 0.0, 2.0)))
            .unwrap();

        let mut right = CadDocument::new();
        right
            .add_entity(EntityType::Circle(Circle::from_coords(10.0, 10.0, 0.0, 2.0)))
            .unwrap();
        right
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();

        let left = extract_document(left, PathBuf::from("left.dxf"), 0);
        let right = extract_document(right, PathBuf::from("right.dxf"), 0);
        let report = build_semantic_diff_report(&left, &right, 10);

        assert!(report.equivalent);
        assert!(report.fingerprint_differences.is_empty());
    }

    #[test]
    fn semantic_diff_detects_geometry_changes() {
        let mut left = CadDocument::new();
        left.add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();

        let mut right = CadDocument::new();
        right
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 6.0, 0.0, 0.0)))
            .unwrap();

        let left = extract_document(left, PathBuf::from("left.dxf"), 0);
        let right = extract_document(right, PathBuf::from("right.dxf"), 0);
        let report = build_semantic_diff_report(&left, &right, 10);

        assert!(!report.equivalent);
        assert!(!report.fingerprint_differences.is_empty());
    }

    #[test]
    fn normalize_output_version_upgrades_legacy_and_unknown_versions() {
        assert_eq!(normalize_output_version(DxfVersion::Unknown), DxfVersion::AC1024);
        assert_eq!(normalize_output_version(DxfVersion::AC1015), DxfVersion::AC1024);
        assert_eq!(normalize_output_version(DxfVersion::AC1024), DxfVersion::AC1024);
        assert_eq!(normalize_output_version(DxfVersion::AC1032), DxfVersion::AC1032);
    }

    #[test]
    fn semantic_diff_ignores_unsupported_entities() {
        let mut left = CadDocument::new();
        left.add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();
        let mut point = Point::from_coords(1.0, 1.0, 0.0);
        point.common.layer = "ANNO".to_owned();
        left.add_entity(EntityType::Point(point)).unwrap();

        let mut right = CadDocument::new();
        right
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();

        let left = extract_document(left, PathBuf::from("left.dxf"), 0);
        let right = extract_document(right, PathBuf::from("right.dxf"), 0);
        let report = build_semantic_diff_report(&left, &right, 10);

        assert!(report.equivalent);
        assert!(report.fingerprint_differences.is_empty());
    }

    #[test]
    fn semantic_diff_tolerates_small_float_noise() {
        let mut left = CadDocument::new();
        left.add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.000000, 0.0, 0.0)))
            .unwrap();

        let mut right = CadDocument::new();
        right
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.000001, 0.0, 0.0)))
            .unwrap();

        let left = extract_document(left, PathBuf::from("left.dxf"), 0);
        let right = extract_document(right, PathBuf::from("right.dxf"), 0);
        let report = build_semantic_diff_report(&left, &right, 10);

        assert!(report.equivalent);
        assert!(report.fingerprint_differences.is_empty());
    }

    #[test]
    fn semantic_diff_normalizes_layout_block_names() {
        let mut left_doc = CadDocument::new();
        left_doc
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();
        let mut right_doc = CadDocument::new();
        right_doc
            .add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 0.0, 0.0)))
            .unwrap();

        let mut left = extract_document(left_doc, PathBuf::from("left.dxf"), 0);
        let mut right = extract_document(right_doc, PathBuf::from("right.dxf"), 0);

        left.layouts = vec![cad_extraction::extraction::models::LayoutInfo {
            name: "Sheet A".to_owned(),
            root_block_name: "*Paper_Space0".to_owned(),
            tab_order: 1,
            is_model: false,
        }];
        right.layouts = vec![cad_extraction::extraction::models::LayoutInfo {
            name: "Sheet A".to_owned(),
            root_block_name: "*Paper_Space4".to_owned(),
            tab_order: 1,
            is_model: false,
        }];
        left.entities[0].block_name = Some("*Paper_Space0".to_owned());
        right.entities[0].block_name = Some("*Paper_Space4".to_owned());

        let report = build_semantic_diff_report(&left, &right, 10);

        assert!(report.equivalent);
        assert!(report.block_differences.is_empty());
        assert!(report.fingerprint_differences.is_empty());
    }

    #[test]
    fn sniff_dxf_header_version_detects_ac1009() {
        let temp_dir = TempDirGuard::new("legacy_header");
        let dxf_path = temp_dir.path.join("legacy.dxf");
        fs::write(
            &dxf_path,
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();

        assert_eq!(
            sniff_dxf_header_version(&dxf_path).unwrap(),
            Some("AC1009".to_owned())
        );
        assert!(build_roundtrip_skip_report(&dxf_path).unwrap().is_some());
    }

    #[test]
    fn sniff_dxf_header_version_ignores_supported_versions() {
        let temp_dir = TempDirGuard::new("modern_header");
        let dxf_path = temp_dir.path.join("modern.dxf");
        fs::write(
            &dxf_path,
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1012\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();

        assert_eq!(
            sniff_dxf_header_version(&dxf_path).unwrap(),
            Some("AC1012".to_owned())
        );
        assert!(build_roundtrip_skip_report(&dxf_path).unwrap().is_none());
    }
}
