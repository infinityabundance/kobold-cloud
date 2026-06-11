# kobold-cloud

The cloud **landing-plan** generator for decoded COBOL datasets. Given a dataset descriptor and a
cloud target, it emits a deterministic deployment/landing **manifest** -- object-store path layout,
catalog table DDL, partition plan, format + compression choice, and an ordered ingestion checklist.
It is the plan a data engineer reviews and applies.

This crate **performs no network I/O**. It plans and emits manifests; it never touches an object
store, a catalog, or a warehouse. Applying a plan (running the DDL, copying the objects) is
deliberately out of scope -- a future, separately-reviewed step. The generator is pure and
deterministic: the same inputs always produce the same manifest.

**Part of KOBOLD** -- a forensic archaeology and evidence system for legacy COBOL estates.
Independently-authored tooling; contains no GnuCOBOL source. Depends only on `serde`/`serde_json`
(no cloud SDK crates).

## Targets

- **S3** -- a raw object-store prefix (`s3://bucket/prefix/...`); no catalog table.
- **AWS Glue** -- a `CREATE EXTERNAL TABLE` over an S3 location.
- **Snowflake** -- a table plus an external stage and a `COPY INTO`.
- **Databricks** -- a Unity Catalog table over an external location.

## Library

```rust
use kobold_cloud::{plan, to_json, Column, Compression, CloudTarget, DatasetDescriptor, Format};

let ds = DatasetDescriptor {
    name: "ACCT-MASTER".into(),
    columns: vec![
        Column::new("acct_id", "string"),
        Column::new("open_date", "date"),
    ],
    estimated_records: 1_000_000,
    source_lrecl: 80,
};
let target = CloudTarget::GlueCatalog { database: "analytics".into(), table: "acct".into() };
let landing = plan(&ds, &target, Format::Parquet, Compression::Snappy);
println!("{}", to_json(&landing, true).unwrap());
```

Partitioning is suggested automatically: the first date-like column (by type or name) becomes a
Hive partition key; otherwise the table is left unpartitioned.

## CLI

```text
kobold-cloud plan <dataset.json> --target <target.json> [--format parquet] \
    [--compression snappy] [--pretty]
```

The target JSON is tagged by `kind`, e.g. `{"kind":"glue-catalog","database":"analytics","table":"acct"}`.

## License

Apache-2.0 (see LICENSE).
