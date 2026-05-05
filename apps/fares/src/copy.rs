use anyhow::{Context, Result};
use crossbeam_channel::{Sender, bounded};
use std::collections::HashMap;
use std::io::Write;
use std::thread::{self, JoinHandle};

use crate::db::connect_with_url;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CopyTarget {
    SourcePayload,
    XmlDocument,
    XmlDocumentRaw,
    NetexObject,
    NetexReference,
    Frame,
    PriceableObject,
    Operator,
    Line,
    ScheduledStopPoint,
    FareZone,
    FareZoneMember,
    Tariff,
    FareProduct,
    SalesOfferPackage,
    SalesOfferPackageElement,
    DistributionAssignment,
    UsageParameter,
    FareTable,
    FareTableAxis,
    DistanceMatrixElement,
    FarePrice,
    DistanceMatrixElementPrice,
    Cell,
    AccessRightParameterAssignment,
    AccessRightParameterAssignmentItem,
    AvailabilityCondition,
    Notice,
    NoticeAssignment,
    QualityIssue,
}

#[derive(Clone)]
pub struct Emitters {
    senders: HashMap<CopyTarget, Sender<CopyRow>>,
}

pub struct CopyManager {
    emitters: Emitters,
    handles: Vec<JoinHandle<Result<()>>>,
}

struct CopySpec {
    target: CopyTarget,
    name: &'static str,
    sql: &'static str,
}

impl CopyManager {
    pub fn start(database_url: String, channel_bound: usize) -> Self {
        let mut senders = HashMap::new();
        let mut handles = Vec::new();

        for spec in COPY_SPECS {
            let (sender, receiver) = bounded::<CopyRow>(channel_bound);
            senders.insert(spec.target, sender);
            let url = database_url.clone();
            let name = spec.name;
            let sql = spec.sql;
            handles.push(thread::spawn(move || -> Result<()> {
                let mut client = connect_with_url(&url)?;
                client
                    .simple_query("SET synchronous_commit = off")
                    .context("failed to disable synchronous_commit for load session")?;
                let mut writer = client
                    .copy_in(sql)
                    .with_context(|| format!("failed to start COPY for {name}"))?;
                match copy_format(spec.target) {
                    CopyFormat::Text => {
                        for row in receiver {
                            let encoded = encode_text_copy_row(&row);
                            writer
                                .write_all(encoded.as_bytes())
                                .with_context(|| format!("failed writing COPY row for {name}"))?;
                        }
                    }
                    CopyFormat::Binary(types) => {
                        writer.write_all(BINARY_COPY_HEADER).with_context(|| {
                            format!("failed writing binary COPY header for {name}")
                        })?;
                        for row in receiver {
                            let encoded =
                                encode_binary_copy_row(&row, types).with_context(|| {
                                    format!("failed encoding binary COPY row for {name}")
                                })?;
                            writer.write_all(&encoded).with_context(|| {
                                format!("failed writing binary COPY row for {name}")
                            })?;
                        }
                        writer.write_all(&(-1_i16).to_be_bytes()).with_context(|| {
                            format!("failed writing binary COPY trailer for {name}")
                        })?;
                    }
                }
                writer
                    .finish()
                    .with_context(|| format!("failed to finish COPY for {name}"))?;
                Ok(())
            }));
        }

        Self {
            emitters: Emitters { senders },
            handles,
        }
    }

    pub fn emitters(&self) -> Emitters {
        self.emitters.clone()
    }

    pub fn finish(self) -> Result<()> {
        drop(self.emitters);
        for handle in self.handles {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("COPY writer thread panicked"))??;
        }
        Ok(())
    }
}

impl Emitters {
    pub fn send(&self, target: CopyTarget, fields: &[Option<String>]) -> Result<()> {
        self.senders
            .get(&target)
            .context("missing COPY sender")?
            .send(fields.to_vec())
            .context("COPY writer stopped before all rows were sent")
    }
}

type CopyRow = Vec<Option<String>>;

pub fn opt<T: ToString>(value: T) -> Option<String> {
    Some(value.to_string())
}

pub fn join_list(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join("\u{1f}"))
    }
}

#[derive(Clone, Copy)]
enum CopyFormat {
    Text,
    Binary(&'static [ColumnType]),
}

#[derive(Clone, Copy)]
enum ColumnType {
    Text,
    Int8,
    Int4,
    Bool,
}

const BINARY_COPY_HEADER: &[u8] = b"PGCOPY\n\xff\r\n\0\0\0\0\0\0\0\0\0";

fn encode_text_copy_row(fields: &[Option<String>]) -> String {
    let mut out = String::new();
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            out.push('\t');
        }
        match field {
            None => out.push_str("\\N"),
            Some(value) => escape_copy_field(value, &mut out),
        }
    }
    out.push('\n');
    out
}

fn escape_copy_field(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
}

fn encode_binary_copy_row(fields: &[Option<String>], types: &[ColumnType]) -> Result<Vec<u8>> {
    if fields.len() != types.len() {
        anyhow::bail!(
            "binary COPY row has {} fields but table expects {}",
            fields.len(),
            types.len()
        );
    }

    let mut out = Vec::with_capacity(fields.len() * 16);
    out.extend_from_slice(&(fields.len() as i16).to_be_bytes());
    for (field, column_type) in fields.iter().zip(types.iter()) {
        match field {
            None => out.extend_from_slice(&(-1_i32).to_be_bytes()),
            Some(value) => {
                let encoded = encode_binary_value(value, *column_type)?;
                out.extend_from_slice(&(encoded.len() as i32).to_be_bytes());
                out.extend_from_slice(&encoded);
            }
        }
    }
    Ok(out)
}

fn encode_binary_value(value: &str, column_type: ColumnType) -> Result<Vec<u8>> {
    match column_type {
        ColumnType::Text => Ok(value.as_bytes().to_vec()),
        ColumnType::Int8 => {
            let parsed = value.parse::<i64>()?;
            Ok(parsed.to_be_bytes().to_vec())
        }
        ColumnType::Int4 => {
            let parsed = value.parse::<i32>()?;
            Ok(parsed.to_be_bytes().to_vec())
        }
        ColumnType::Bool => {
            let parsed = match value.to_ascii_lowercase().as_str() {
                "true" | "t" | "1" | "yes" => true,
                "false" | "f" | "0" | "no" => false,
                _ => anyhow::bail!("invalid boolean value for binary COPY: {value}"),
            };
            Ok(vec![u8::from(parsed)])
        }
    }
}

fn copy_format(target: CopyTarget) -> CopyFormat {
    match target {
        CopyTarget::NetexObject => CopyFormat::Binary(NETEX_OBJECT_TYPES),
        CopyTarget::NetexReference => CopyFormat::Binary(NETEX_REFERENCE_TYPES),
        CopyTarget::DistanceMatrixElement => CopyFormat::Binary(DISTANCE_MATRIX_ELEMENT_TYPES),
        CopyTarget::FarePrice => CopyFormat::Binary(FARE_PRICE_TYPES),
        CopyTarget::DistanceMatrixElementPrice => {
            CopyFormat::Binary(DISTANCE_MATRIX_ELEMENT_PRICE_TYPES)
        }
        CopyTarget::Cell => CopyFormat::Binary(CELL_TYPES),
        _ => CopyFormat::Text,
    }
}

const NETEX_OBJECT_TYPES: &[ColumnType] = &[
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int4,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
];

const NETEX_REFERENCE_TYPES: &[ColumnType] = &[
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
];

const DISTANCE_MATRIX_ELEMENT_TYPES: &[ColumnType] = &[
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
];

const FARE_PRICE_TYPES: &[ColumnType] = &[
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Bool,
    ColumnType::Text,
    ColumnType::Int8,
];

const DISTANCE_MATRIX_ELEMENT_PRICE_TYPES: &[ColumnType] = &[
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Bool,
    ColumnType::Text,
    ColumnType::Text,
];

const CELL_TYPES: &[ColumnType] = &[
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Int8,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Text,
    ColumnType::Bool,
];

static COPY_SPECS: &[CopySpec] = &[
    CopySpec {
        target: CopyTarget::SourcePayload,
        name: "source_payload",
        sql: "COPY fare_staging.load_source_payload FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::XmlDocument,
        name: "xml_document",
        sql: "COPY fare_staging.load_xml_document FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::XmlDocumentRaw,
        name: "xml_document_raw",
        sql: "COPY fare_staging.load_xml_document_raw FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::NetexObject,
        name: "netex_object",
        sql: "COPY fare_staging.load_netex_object FROM STDIN WITH (FORMAT binary)",
    },
    CopySpec {
        target: CopyTarget::NetexReference,
        name: "netex_reference",
        sql: "COPY fare_staging.load_netex_reference FROM STDIN WITH (FORMAT binary)",
    },
    CopySpec {
        target: CopyTarget::Frame,
        name: "frame",
        sql: "COPY fare_staging.load_frame FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::PriceableObject,
        name: "priceable_object",
        sql: "COPY fare_staging.load_priceable_object FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::Operator,
        name: "operator",
        sql: "COPY fare_staging.load_operator FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::Line,
        name: "line",
        sql: "COPY fare_staging.load_line FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::ScheduledStopPoint,
        name: "scheduled_stop_point",
        sql: "COPY fare_staging.load_scheduled_stop_point FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::FareZone,
        name: "fare_zone",
        sql: "COPY fare_staging.load_fare_zone FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::FareZoneMember,
        name: "fare_zone_member",
        sql: "COPY fare_staging.load_fare_zone_member FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::Tariff,
        name: "tariff",
        sql: "COPY fare_staging.load_tariff FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::FareProduct,
        name: "fare_product",
        sql: "COPY fare_staging.load_fare_product FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::SalesOfferPackage,
        name: "sales_offer_package",
        sql: "COPY fare_staging.load_sales_offer_package FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::SalesOfferPackageElement,
        name: "sales_offer_package_element",
        sql: "COPY fare_staging.load_sales_offer_package_element FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::DistributionAssignment,
        name: "distribution_assignment",
        sql: "COPY fare_staging.load_distribution_assignment FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::UsageParameter,
        name: "usage_parameter",
        sql: "COPY fare_staging.load_usage_parameter FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::FareTable,
        name: "fare_table",
        sql: "COPY fare_staging.load_fare_table FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::FareTableAxis,
        name: "fare_table_axis",
        sql: "COPY fare_staging.load_fare_table_axis FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::DistanceMatrixElement,
        name: "distance_matrix_element",
        sql: "COPY fare_staging.load_distance_matrix_element FROM STDIN WITH (FORMAT binary)",
    },
    CopySpec {
        target: CopyTarget::FarePrice,
        name: "fare_price",
        sql: "COPY fare_staging.load_fare_price FROM STDIN WITH (FORMAT binary)",
    },
    CopySpec {
        target: CopyTarget::DistanceMatrixElementPrice,
        name: "distance_matrix_element_price",
        sql: "COPY fare_staging.load_distance_matrix_element_price FROM STDIN WITH (FORMAT binary)",
    },
    CopySpec {
        target: CopyTarget::Cell,
        name: "cell",
        sql: "COPY fare_staging.load_cell FROM STDIN WITH (FORMAT binary)",
    },
    CopySpec {
        target: CopyTarget::AccessRightParameterAssignment,
        name: "access_right_parameter_assignment",
        sql: "COPY fare_staging.load_access_right_parameter_assignment FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::AccessRightParameterAssignmentItem,
        name: "access_right_parameter_assignment_item",
        sql: "COPY fare_staging.load_access_right_parameter_assignment_item FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::AvailabilityCondition,
        name: "availability_condition",
        sql: "COPY fare_staging.load_availability_condition FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::Notice,
        name: "notice",
        sql: "COPY fare_staging.load_notice FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::NoticeAssignment,
        name: "notice_assignment",
        sql: "COPY fare_staging.load_notice_assignment FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
    CopySpec {
        target: CopyTarget::QualityIssue,
        name: "quality_issue",
        sql: "COPY fare_staging.import_quality_issue (batch_id, severity, issue_code, issue_count, source_object_id, source_document_uid, details) FROM STDIN WITH (FORMAT text, NULL '\\N')",
    },
];
