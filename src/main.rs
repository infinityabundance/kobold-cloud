//! `kobold-cloud` CLI: generate a deterministic cloud landing plan (manifest) for a decoded COBOL
//! dataset and print it as JSON. Plans only -- this tool performs no network I/O.
//!
//! Usage:
//!   kobold-cloud plan <dataset.json> --target <target.json> [--format parquet] \
//!       [--compression snappy] [--pretty]
//!
//! `<dataset.json>` matches [`kobold_cloud::DatasetDescriptor`], e.g.:
//!   {"name":"ACCT-MASTER","estimated_records":1000000,"source_lrecl":80,
//!    "columns":[{"name":"acct_id","logical_type":"string"},
//!               {"name":"open_date","logical_type":"date"}]}
//!
//! `<target.json>` matches [`kobold_cloud::CloudTarget`] (tagged by `kind`), e.g.:
//!   {"kind":"glue-catalog","database":"analytics","table":"acct"}
#![forbid(unsafe_code)]

use kobold_cloud::{plan, to_json, CloudTarget, Compression, DatasetDescriptor, Format};
use std::process::ExitCode;

fn parse_format(s: &str) -> Option<Format> {
    match s.to_ascii_lowercase().as_str() {
        "parquet" => Some(Format::Parquet),
        "avro" => Some(Format::Avro),
        "csv" => Some(Format::Csv),
        "json" => Some(Format::Json),
        _ => None,
    }
}

fn parse_compression(s: &str) -> Option<Compression> {
    match s.to_ascii_lowercase().as_str() {
        "snappy" => Some(Compression::Snappy),
        "gzip" => Some(Compression::Gzip),
        "zstd" => Some(Compression::Zstd),
        "none" => Some(Compression::None),
        _ => None,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] != "plan" {
        eprintln!(
            "usage: kobold-cloud plan <dataset.json> --target <target.json> \
             [--format parquet] [--compression snappy] [--pretty]"
        );
        return ExitCode::from(2);
    }

    let mut dataset_path: Option<String> = None;
    let mut target_path: Option<String> = None;
    let mut format = Format::Parquet;
    let mut compression = Compression::Snappy;
    let mut pretty = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                i += 1;
                target_path = args.get(i).cloned();
            }
            "--format" => {
                i += 1;
                match args.get(i).and_then(|s| parse_format(s)) {
                    Some(f) => format = f,
                    None => {
                        eprintln!("error: --format must be one of parquet|avro|csv|json");
                        return ExitCode::from(2);
                    }
                }
            }
            "--compression" => {
                i += 1;
                match args.get(i).and_then(|s| parse_compression(s)) {
                    Some(c) => compression = c,
                    None => {
                        eprintln!("error: --compression must be one of snappy|gzip|zstd|none");
                        return ExitCode::from(2);
                    }
                }
            }
            "--pretty" => pretty = true,
            other => dataset_path = Some(other.to_string()),
        }
        i += 1;
    }

    let (Some(dp), Some(tp)) = (dataset_path, target_path) else {
        eprintln!("error: need both <dataset.json> and --target <target.json>");
        return ExitCode::from(2);
    };

    let dataset_src = match std::fs::read_to_string(&dp) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read dataset {dp}: {e}");
            return ExitCode::from(2);
        }
    };
    let target_src = match std::fs::read_to_string(&tp) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read target {tp}: {e}");
            return ExitCode::from(2);
        }
    };

    let dataset: DatasetDescriptor = match serde_json::from_str(&dataset_src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: invalid dataset JSON: {e}");
            return ExitCode::from(2);
        }
    };
    let target: CloudTarget = match serde_json::from_str(&target_src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: invalid target JSON: {e}");
            return ExitCode::from(2);
        }
    };

    let landing = plan(&dataset, &target, format, compression);
    match to_json(&landing, pretty) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: serialize plan: {e}");
            ExitCode::from(2)
        }
    }
}
