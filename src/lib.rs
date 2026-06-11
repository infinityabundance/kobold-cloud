//! # kobold-cloud
//!
//! The cloud **landing-plan** generator for decoded COBOL datasets. Given a dataset descriptor
//! (name, record-layout summary, estimated record count, source LRECL) and a cloud target, it
//! emits a deterministic deployment/landing **manifest**: object-store path layout, catalog table
//! definition (DDL), partitioning plan, format + compression choice, and an ordered ingestion-step
//! checklist -- the plan a data engineer reviews and applies.
//!
//! This crate **performs no network I/O**. It plans and emits manifests; it never touches an
//! object store, a catalog, or a warehouse. Applying a plan (running the DDL, copying the objects)
//! is deliberately out of scope -- a future, separately-reviewed step. Everything here is pure and
//! deterministic: the same inputs always produce the same manifest.
//!
//! Part of the KOBOLD ecosystem -- independently-authored forensic tooling, Apache-2.0. This crate
//! contains **no GnuCOBOL/libcob source** and depends only on `serde`/`serde_json`. The cloud
//! conventions it templates (Hive partitioning, external tables, object-store key layout) are
//! public, long-documented data-platform patterns.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Where a decoded dataset is to be landed. Each variant carries only the identity coordinates of
/// the destination -- never credentials, endpoints, or anything that would imply a live connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
#[non_exhaustive]
pub enum CloudTarget {
    /// A raw object-store prefix (Amazon S3 / S3-compatible).
    S3 { bucket: String, prefix: String },
    /// An AWS Glue Data Catalog table backed by an external S3 location.
    GlueCatalog { database: String, table: String },
    /// A Snowflake table loaded via an external stage + `COPY INTO`.
    Snowflake { database: String, schema: String, table: String },
    /// A Databricks Unity Catalog table over an external location.
    Databricks { catalog: String, schema: String, table: String },
}

/// On-disk file format for the landed objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Format {
    Parquet,
    Avro,
    Csv,
    Json,
}

impl Format {
    /// The conventional file extension (without a leading dot).
    pub fn extension(self) -> &'static str {
        match self {
            Format::Parquet => "parquet",
            Format::Avro => "avro",
            Format::Csv => "csv",
            Format::Json => "json",
        }
    }
    /// The token used in catalog DDL `STORED AS` / `FILEFORMAT` clauses.
    pub fn ddl_token(self) -> &'static str {
        match self {
            Format::Parquet => "PARQUET",
            Format::Avro => "AVRO",
            Format::Csv => "CSV",
            Format::Json => "JSON",
        }
    }
}

/// Compression codec applied to the landed objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Compression {
    Snappy,
    Gzip,
    Zstd,
    None,
}

impl Compression {
    /// The codec token used in DDL / `COPY` options. `None` => `"NONE"`.
    pub fn token(self) -> &'static str {
        match self {
            Compression::Snappy => "SNAPPY",
            Compression::Gzip => "GZIP",
            Compression::Zstd => "ZSTD",
            Compression::None => "NONE",
        }
    }
}

/// One logical column of the decoded dataset. `logical_type` is a platform-neutral type name
/// (e.g. `"string"`, `"decimal(9,2)"`, `"date"`, `"int"`) that the per-target DDL renderer maps to
/// a concrete SQL type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub logical_type: String,
}

impl Column {
    pub fn new(name: impl Into<String>, logical_type: impl Into<String>) -> Self {
        Column { name: name.into(), logical_type: logical_type.into() }
    }
}

/// A summary of the decoded dataset to be landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetDescriptor {
    pub name: String,
    #[serde(default)]
    pub columns: Vec<Column>,
    /// Operator's estimate of the row count -- used only to size the ingestion checklist guidance.
    #[serde(default)]
    pub estimated_records: u64,
    /// Logical record length of the source fixed-record COBOL dataset, in bytes (provenance only).
    #[serde(default)]
    pub source_lrecl: usize,
}

/// A partitioning plan for the landed table. An empty `keys` list means an unpartitioned table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionPlan {
    pub keys: Vec<String>,
    /// Partition layout scheme, e.g. `"hive"` (`key=value` directories) or `"none"`.
    pub scheme: String,
}

impl PartitionPlan {
    /// An unpartitioned plan.
    pub fn none() -> Self {
        PartitionPlan { keys: Vec::new(), scheme: "none".to_string() }
    }

    /// `true` when no partition keys are present.
    pub fn is_unpartitioned(&self) -> bool {
        self.keys.is_empty()
    }

    /// Suggest a partitioning plan for a dataset. Heuristic: partition by the first column whose
    /// logical type or name looks date-like (a `date`/`timestamp` type, or a name containing
    /// `date`/`dt`/`day`/`month`/`year`), under the Hive `key=value` scheme. If no such column
    /// exists, return an unpartitioned plan. Deterministic: scans columns in declared order.
    pub fn suggest(dataset: &DatasetDescriptor) -> Self {
        for col in &dataset.columns {
            if is_date_like(col) {
                return PartitionPlan { keys: vec![col.name.clone()], scheme: "hive".to_string() };
            }
        }
        PartitionPlan::none()
    }
}

/// Whether a column looks like a partition-worthy date/time key.
fn is_date_like(col: &Column) -> bool {
    let ty = col.logical_type.to_ascii_lowercase();
    if ty.starts_with("date") || ty.starts_with("timestamp") || ty.starts_with("datetime") {
        return true;
    }
    let name = col.name.to_ascii_lowercase();
    const TOKENS: &[&str] = &["date", "_dt", "dt_", "day", "month", "year", "_ts", "ts_"];
    name == "dt" || name == "ts" || TOKENS.iter().any(|t| name.contains(t))
}

/// The emitted landing manifest: everything a data engineer needs to review before applying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandingPlan {
    pub target: CloudTarget,
    pub format: Format,
    pub compression: Compression,
    /// S3-style object-store path (or stage path) under which the dataset's objects are landed.
    pub object_path: String,
    /// The catalog DDL appropriate to the target (external table, stage + copy, etc.).
    pub table_ddl: String,
    pub partitions: PartitionPlan,
    /// The ordered ingestion checklist -- the steps an operator follows to apply this plan.
    pub steps: Vec<String>,
}

/// Map a platform-neutral logical type to a concrete SQL type for a given target family.
fn sql_type(logical: &str, target: &CloudTarget) -> String {
    let l = logical.trim().to_ascii_lowercase();
    // Decimal / numeric carry their precision through unchanged where the dialect supports it.
    if l.starts_with("decimal") || l.starts_with("numeric") {
        return l.to_uppercase();
    }
    match (&l[..], target) {
        ("string" | "char" | "varchar" | "text", CloudTarget::Snowflake { .. }) => "VARCHAR".into(),
        ("string" | "char" | "varchar" | "text", _) => "STRING".into(),
        ("int" | "integer" | "int32", CloudTarget::Snowflake { .. }) => "NUMBER(38,0)".into(),
        ("int" | "integer" | "int32", _) => "INT".into(),
        ("long" | "bigint" | "int64", CloudTarget::Snowflake { .. }) => "NUMBER(38,0)".into(),
        ("long" | "bigint" | "int64", _) => "BIGINT".into(),
        ("date", _) => "DATE".into(),
        ("timestamp" | "datetime", _) => "TIMESTAMP".into(),
        ("bool" | "boolean", _) => "BOOLEAN".into(),
        ("double" | "float" | "real", _) => "DOUBLE".into(),
        // Unknown logical types pass through uppercased so the engineer can correct them.
        _ => l.to_uppercase(),
    }
}

/// Render the column list for a `CREATE TABLE` body, indented, one column per line.
fn render_columns(dataset: &DatasetDescriptor, target: &CloudTarget, partition_keys: &[String]) -> String {
    let cols: Vec<String> = dataset
        .columns
        .iter()
        .filter(|c| !partition_keys.contains(&c.name))
        .map(|c| format!("  {} {}", c.name, sql_type(&c.logical_type, target)))
        .collect();
    cols.join(",\n")
}

/// Normalise a name into a safe lowercase object-store path segment.
fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let trimmed = s.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "dataset".to_string()
    } else {
        trimmed
    }
}

/// Join a prefix and a segment with exactly one separating slash, trimming stray slashes.
fn join_path(prefix: &str, segment: &str) -> String {
    let p = prefix.trim_matches('/');
    let s = segment.trim_matches('/');
    match (p.is_empty(), s.is_empty()) {
        (true, _) => s.to_string(),
        (false, true) => p.to_string(),
        (false, false) => format!("{p}/{s}"),
    }
}

/// Compute the object-store landing path (an `s3://...` URI for object/catalog targets, or a
/// stage-relative path for Snowflake) for a dataset under a target.
fn object_path(dataset: &DatasetDescriptor, target: &CloudTarget) -> String {
    let ds = slug(&dataset.name);
    match target {
        CloudTarget::S3 { bucket, prefix } => {
            let key = join_path(prefix, &ds);
            format!("s3://{}/{}/", bucket, key)
        }
        CloudTarget::GlueCatalog { database, table } => {
            // Glue external tables live under a conventional warehouse layout.
            format!("s3://{}-warehouse/{}/{}/", slug(database), slug(database), slug(table))
        }
        CloudTarget::Snowflake { database, schema, table } => {
            // Stage-relative path under the named internal/external stage.
            format!("@{}.{}.{}_stage/{}/", database, schema, table, ds)
        }
        CloudTarget::Databricks { catalog, schema, table } => {
            format!("s3://{}-external/{}/{}/{}/", slug(catalog), slug(catalog), slug(schema), slug(table))
        }
    }
}

/// Render the catalog DDL for a target. Templated but realistic; deterministic for fixed inputs.
fn table_ddl(
    dataset: &DatasetDescriptor,
    target: &CloudTarget,
    format: Format,
    compression: Compression,
    object_path: &str,
    partitions: &PartitionPlan,
) -> String {
    let cols = render_columns(dataset, target, &partitions.keys);
    let part_cols: Vec<String> = partitions
        .keys
        .iter()
        .filter_map(|k| dataset.columns.iter().find(|c| &c.name == k))
        .map(|c| format!("{} {}", c.name, sql_type(&c.logical_type, target)))
        .collect();

    match target {
        CloudTarget::S3 { .. } => {
            // Raw object store: no catalog. Emit a descriptive manifest comment instead of DDL.
            format!(
                "-- raw object-store landing for '{}' ({} objects, {} compression) at {}\n\
                 -- no catalog table is created for an S3-only target; register it in a catalog separately",
                dataset.name,
                format.ddl_token(),
                compression.token(),
                object_path
            )
        }
        CloudTarget::GlueCatalog { database, table } => {
            let mut ddl = format!(
                "CREATE EXTERNAL TABLE IF NOT EXISTS {database}.{table} (\n{cols}\n)"
            );
            if !part_cols.is_empty() {
                ddl.push_str(&format!("\nPARTITIONED BY ({})", part_cols.join(", ")));
            }
            ddl.push_str(&format!(
                "\nSTORED AS {}\nLOCATION '{}'\nTBLPROPERTIES ('compression'='{}');",
                format.ddl_token(),
                object_path,
                compression.token().to_lowercase()
            ));
            ddl
        }
        CloudTarget::Snowflake { database, schema, table } => {
            let part = if partitions.is_unpartitioned() {
                String::new()
            } else {
                format!("-- suggested clustering keys: {}\n", partitions.keys.join(", "))
            };
            format!(
                "CREATE TABLE IF NOT EXISTS {database}.{schema}.{table} (\n{cols}\n);\n\
                 CREATE STAGE IF NOT EXISTS {database}.{schema}.{table}_stage;\n\
                 {part}COPY INTO {database}.{schema}.{table}\n\
                 FROM '{object_path}'\n\
                 FILE_FORMAT = (TYPE = {} COMPRESSION = {});",
                format.ddl_token(),
                compression.token()
            )
        }
        CloudTarget::Databricks { catalog, schema, table } => {
            let mut ddl = format!(
                "CREATE TABLE IF NOT EXISTS {catalog}.{schema}.{table} (\n{cols}\n)\nUSING {}",
                format.ddl_token()
            );
            if !part_cols.is_empty() {
                let names: Vec<&str> = partitions.keys.iter().map(String::as_str).collect();
                ddl.push_str(&format!("\nPARTITIONED BY ({})", names.join(", ")));
            }
            ddl.push_str(&format!(
                "\nLOCATION '{}'\nTBLPROPERTIES ('compression'='{}');",
                object_path,
                compression.token().to_lowercase()
            ));
            ddl
        }
    }
}

/// Build the ordered ingestion checklist for a target.
fn steps(
    dataset: &DatasetDescriptor,
    target: &CloudTarget,
    format: Format,
    compression: Compression,
    object_path: &str,
    partitions: &PartitionPlan,
) -> Vec<String> {
    let mut steps = Vec::new();
    steps.push(format!(
        "Encode the {} decoded records of '{}' as {} with {} compression.",
        dataset.estimated_records,
        dataset.name,
        format.ddl_token(),
        compression.token()
    ));
    if !partitions.is_unpartitioned() {
        steps.push(format!(
            "Lay the objects out under the {} partition scheme keyed by [{}].",
            partitions.scheme,
            partitions.keys.join(", ")
        ));
    }
    steps.push(format!("Stage the objects under {object_path}."));

    match target {
        CloudTarget::S3 { .. } => {
            steps.push("Upload the objects to the S3 prefix.".to_string());
            steps.push("Verify object count and total byte size against the source estimate.".to_string());
        }
        CloudTarget::GlueCatalog { database, table } => {
            steps.push("Upload the objects to the external S3 location.".to_string());
            steps.push(format!("Run the CREATE EXTERNAL TABLE DDL to register {database}.{table} in Glue."));
            if !partitions.is_unpartitioned() {
                steps.push("Run MSCK REPAIR TABLE (or ADD PARTITION) to discover partitions.".to_string());
            }
            steps.push("Validate with a SELECT COUNT(*) against the estimated record count.".to_string());
        }
        CloudTarget::Snowflake { table, .. } => {
            steps.push(format!("Create the {table}_stage stage and upload the objects to it."));
            steps.push("Run the CREATE TABLE DDL.".to_string());
            steps.push("Run COPY INTO and inspect the load result for rejected rows.".to_string());
            steps.push("Validate row count against the estimate.".to_string());
        }
        CloudTarget::Databricks { catalog, schema, table } => {
            steps.push("Upload the objects to the external location.".to_string());
            steps.push(format!("Run the CREATE TABLE DDL to register {catalog}.{schema}.{table}."));
            steps.push("Validate with SELECT COUNT(*) against the estimated record count.".to_string());
        }
    }
    steps
}

/// Build a deterministic [`LandingPlan`] for a dataset, target, format, and compression. Pure: no
/// network I/O, no filesystem access -- it computes the manifest and returns it.
pub fn plan(
    dataset: &DatasetDescriptor,
    target: &CloudTarget,
    format: Format,
    compression: Compression,
) -> LandingPlan {
    let partitions = PartitionPlan::suggest(dataset);
    let object_path = object_path(dataset, target);
    let table_ddl = table_ddl(dataset, target, format, compression, &object_path, &partitions);
    let steps = steps(dataset, target, format, compression, &object_path, &partitions);
    LandingPlan {
        target: target.clone(),
        format,
        compression,
        object_path,
        table_ddl,
        partitions,
        steps,
    }
}

/// Serialize a landing plan to JSON. `pretty` selects multi-line output.
pub fn to_json(plan: &LandingPlan, pretty: bool) -> Result<String, serde_json::Error> {
    if pretty {
        serde_json::to_string_pretty(plan)
    } else {
        serde_json::to_string(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DatasetDescriptor {
        DatasetDescriptor {
            name: "ACCT-MASTER".to_string(),
            columns: vec![
                Column::new("acct_id", "string"),
                Column::new("balance", "decimal(11,2)"),
                Column::new("open_date", "date"),
            ],
            estimated_records: 1_000_000,
            source_lrecl: 80,
        }
    }

    #[test]
    fn s3_target_object_path_under_bucket_prefix() {
        let ds = sample();
        let target = CloudTarget::S3 { bucket: "my-lake".into(), prefix: "raw/cobol".into() };
        let p = plan(&ds, &target, Format::Parquet, Compression::Snappy);
        assert!(p.object_path.starts_with("s3://my-lake/raw/cobol/"), "{}", p.object_path);
        assert!(p.object_path.contains("acct_master"), "{}", p.object_path);
        // raw S3 has no catalog DDL
        assert!(p.table_ddl.contains("no catalog table"));
    }

    #[test]
    fn s3_path_normalises_slashes() {
        let ds = sample();
        let target = CloudTarget::S3 { bucket: "lake".into(), prefix: "/raw//".into() };
        let p = plan(&ds, &target, Format::Json, Compression::None);
        assert_eq!(p.object_path, "s3://lake/raw/acct_master/");
    }

    #[test]
    fn glue_target_emits_external_table_ddl_with_columns() {
        let ds = sample();
        let target = CloudTarget::GlueCatalog { database: "analytics".into(), table: "acct".into() };
        let p = plan(&ds, &target, Format::Parquet, Compression::Zstd);
        assert!(p.table_ddl.contains("CREATE EXTERNAL TABLE"), "{}", p.table_ddl);
        assert!(p.table_ddl.contains("analytics.acct"));
        assert!(p.table_ddl.contains("acct_id"));
        assert!(p.table_ddl.contains("balance"));
        assert!(p.table_ddl.contains("STORED AS PARQUET"));
        assert!(p.table_ddl.contains("LOCATION 's3://"));
        // partition column moves into PARTITIONED BY, not the body
        assert!(p.table_ddl.contains("PARTITIONED BY (open_date DATE)"));
    }

    #[test]
    fn partition_suggestion_picks_date_like_column() {
        let ds = sample();
        let pp = PartitionPlan::suggest(&ds);
        assert_eq!(pp.keys, vec!["open_date".to_string()]);
        assert_eq!(pp.scheme, "hive");
        assert!(!pp.is_unpartitioned());
    }

    #[test]
    fn partition_suggestion_by_type_when_name_is_plain() {
        let ds = DatasetDescriptor {
            name: "events".into(),
            columns: vec![Column::new("id", "int"), Column::new("seen", "timestamp")],
            estimated_records: 10,
            source_lrecl: 20,
        };
        let pp = PartitionPlan::suggest(&ds);
        assert_eq!(pp.keys, vec!["seen".to_string()]);
    }

    #[test]
    fn no_partition_when_no_date_column() {
        let ds = DatasetDescriptor {
            name: "codes".into(),
            columns: vec![Column::new("code", "string"), Column::new("qty", "int")],
            estimated_records: 5,
            source_lrecl: 8,
        };
        let pp = PartitionPlan::suggest(&ds);
        assert!(pp.is_unpartitioned());
        assert_eq!(pp.scheme, "none");
    }

    #[test]
    fn snowflake_target_emits_stage_and_copy() {
        let ds = sample();
        let target = CloudTarget::Snowflake {
            database: "DB".into(),
            schema: "PUBLIC".into(),
            table: "ACCT".into(),
        };
        let p = plan(&ds, &target, Format::Csv, Compression::Gzip);
        assert!(p.table_ddl.contains("CREATE TABLE IF NOT EXISTS DB.PUBLIC.ACCT"));
        assert!(p.table_ddl.contains("CREATE STAGE IF NOT EXISTS DB.PUBLIC.ACCT_stage"));
        assert!(p.table_ddl.contains("COPY INTO DB.PUBLIC.ACCT"));
        assert!(p.table_ddl.contains("TYPE = CSV"));
        assert!(p.table_ddl.contains("COMPRESSION = GZIP"));
        assert!(p.object_path.starts_with("@DB.PUBLIC.ACCT_stage/"));
        // string maps to VARCHAR for Snowflake
        assert!(p.table_ddl.contains("acct_id VARCHAR"));
    }

    #[test]
    fn databricks_target_emits_using_and_location() {
        let ds = sample();
        let target = CloudTarget::Databricks {
            catalog: "main".into(),
            schema: "bronze".into(),
            table: "acct".into(),
        };
        let p = plan(&ds, &target, Format::Parquet, Compression::Snappy);
        assert!(p.table_ddl.contains("CREATE TABLE IF NOT EXISTS main.bronze.acct"));
        assert!(p.table_ddl.contains("USING PARQUET"));
        assert!(p.table_ddl.contains("PARTITIONED BY (open_date)"));
        assert!(p.table_ddl.contains("LOCATION 's3://main-external/"));
    }

    #[test]
    fn steps_are_ordered_and_nonempty() {
        let ds = sample();
        let target = CloudTarget::GlueCatalog { database: "d".into(), table: "t".into() };
        let p = plan(&ds, &target, Format::Parquet, Compression::Snappy);
        assert!(!p.steps.is_empty());
        assert!(p.steps[0].contains("Encode"));
        assert!(p.steps.iter().any(|s| s.contains("CREATE EXTERNAL TABLE DDL")));
        assert!(p.steps.iter().any(|s| s.contains("MSCK REPAIR")));
    }

    #[test]
    fn plan_is_deterministic() {
        let ds = sample();
        let target = CloudTarget::GlueCatalog { database: "d".into(), table: "t".into() };
        let a = plan(&ds, &target, Format::Avro, Compression::Zstd);
        let b = plan(&ds, &target, Format::Avro, Compression::Zstd);
        assert_eq!(a, b);
    }

    #[test]
    fn json_round_trip() {
        let ds = sample();
        let target = CloudTarget::Databricks {
            catalog: "main".into(),
            schema: "bronze".into(),
            table: "acct".into(),
        };
        let p = plan(&ds, &target, Format::Parquet, Compression::Zstd);
        let json = to_json(&p, true).expect("serialize");
        let back: LandingPlan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn dataset_descriptor_round_trip() {
        let ds = sample();
        let json = serde_json::to_string(&ds).unwrap();
        let back: DatasetDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(ds, back);
    }

    #[test]
    fn format_and_compression_tokens() {
        assert_eq!(Format::Parquet.ddl_token(), "PARQUET");
        assert_eq!(Format::Csv.extension(), "csv");
        assert_eq!(Compression::None.token(), "NONE");
        assert_eq!(Compression::Zstd.token(), "ZSTD");
    }
}
