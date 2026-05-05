use anyhow::{Context, Result};
use bumpalo::Bump;
use crossbeam_channel::bounded;
use flate2::Compression;
use flate2::write::GzEncoder;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::thread;
use zip::ZipArchive;

use crate::config::Config;
use crate::copy::{CopyTarget, Emitters, join_list, opt};
use crate::discovery::DocumentSource;

const PRICEABLE_ELEMENTS: &[&str] = &[
    "FareStructureElement",
    "DistanceMatrixElement",
    "GeographicalInterval",
    "TimeInterval",
    "UsageParameter",
    "UserProfile",
    "CompanionProfile",
    "CommercialProfile",
    "SalesOfferPackage",
    "SalesOfferPackageElement",
    "PreassignedFareProduct",
    "AmountOfPriceUnitProduct",
    "SupplementProduct",
    "ThirdPartyProduct",
    "UsageDiscountRight",
    "SaleDiscountRight",
    "CappedDiscountRight",
    "CappingRule",
    "ControllableElement",
];

const FARE_PRODUCT_ELEMENTS: &[&str] = &[
    "PreassignedFareProduct",
    "AmountOfPriceUnitProduct",
    "SupplementProduct",
    "ThirdPartyProduct",
    "UsageDiscountRight",
    "SaleDiscountRight",
    "CappedDiscountRight",
];

const FARE_PRICE_ELEMENTS: &[&str] = &[
    "FarePrice",
    "CellPrice",
    "DistanceMatrixElementPrice",
    "GeographicalIntervalPrice",
    "TimeIntervalPrice",
    "FareStructureElementPrice",
    "FareProductPrice",
    "SalesOfferPackagePrice",
    "UsageParameterPrice",
    "FulfilmentMethodPrice",
    "QualityStructureFactorPrice",
    "ValidableElementPrice",
    "ControllableElementPrice",
    "CappingRulePrice",
];

#[derive(Clone)]
struct ObjectCtx {
    uid: u64,
    ordinal: u32,
    element: String,
    netex_id: String,
    version: Option<String>,
    parent_uid: Option<u64>,
    frame_uid: Option<u64>,
    fare_table_uid: Option<u64>,
    sales_offer_package_uid: Option<u64>,
    parent_axis_uid: Option<u64>,
    frame_currency: Option<String>,
    path: String,
    attrs: HashMap<String, String>,
    fields: HashMap<String, String>,
    lists: HashMap<String, Vec<String>>,
}

#[derive(Clone)]
struct ReferenceRow {
    batch_id: i64,
    document_uid: u64,
    source_object_id: Option<u64>,
    source_element_name: Option<String>,
    ref_element_name: String,
    target_element_name: Option<String>,
    target_ref: String,
    target_version: Option<String>,
    text_value: Option<String>,
    resolved_object_id: Option<u64>,
}

struct DocumentRows {
    rows: Vec<(CopyTarget, Vec<Option<String>>)>,
    references: Vec<ReferenceRow>,
    object_index: HashMap<String, u64>,
}

struct XmlDocumentStatus {
    sha256_hex: Option<String>,
    parse_status: &'static str,
    parse_error: Option<String>,
    root_name: Option<String>,
    namespace: Option<String>,
    store_raw_xml: bool,
}

impl DocumentRows {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            references: Vec::new(),
            object_index: HashMap::new(),
        }
    }

    fn push(&mut self, target: CopyTarget, fields: Vec<Option<String>>) {
        self.rows.push((target, fields));
    }

    fn flush(mut self, emitters: &Emitters) -> Result<()> {
        for reference in &mut self.references {
            if reference.resolved_object_id.is_none() {
                let key = object_key(
                    reference.target_element_name.as_deref().unwrap_or(""),
                    &reference.target_ref,
                    reference.target_version.as_deref(),
                );
                reference.resolved_object_id = self.object_index.get(&key).copied();
            }
            if reference.resolved_object_id.is_none() {
                let key = object_key(
                    "",
                    &reference.target_ref,
                    reference.target_version.as_deref(),
                );
                reference.resolved_object_id = self.object_index.get(&key).copied();
            }
            let status = if reference.resolved_object_id.is_some() {
                "resolved_same_document"
            } else {
                "unresolved"
            };
            emitters.send(
                CopyTarget::NetexReference,
                &[
                    opt(reference.batch_id),
                    opt(reference.document_uid),
                    reference.source_object_id.map(|v| v.to_string()),
                    reference.source_element_name.clone(),
                    opt(reference.ref_element_name.clone()),
                    reference.target_element_name.clone(),
                    opt(reference.target_ref.clone()),
                    reference.target_version.clone(),
                    reference.text_value.clone(),
                    reference.resolved_object_id.map(|v| v.to_string()),
                    opt(status),
                ],
            )?;
        }

        for (target, fields) in self.rows {
            emitters.send(target, &fields)?;
        }
        Ok(())
    }
}

pub fn parse_documents(
    config: &Config,
    documents: Vec<DocumentSource>,
    emitters: Emitters,
) -> Result<()> {
    let worker_count = config.workers.min(documents.len().max(1));
    let (sender, receiver) = bounded::<DocumentSource>(worker_count * 2);
    let mut handles = Vec::new();

    for _ in 0..worker_count {
        let receiver = receiver.clone();
        let emitters = emitters.clone();
        let config = config.clone();
        handles.push(thread::spawn(move || -> Result<()> {
            for document in receiver {
                parse_one_document(&config, &document, &emitters)
                    .with_context(|| format!("failed to parse {}", document.document_path))?;
            }
            Ok(())
        }));
    }

    for document in documents {
        sender.send(document).context("parser worker stopped")?;
    }
    drop(sender);

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("parser worker panicked"))??;
    }

    Ok(())
}

pub fn emit_skipped_documents(emitters: &Emitters, documents: &[DocumentSource]) -> Result<()> {
    for document in documents {
        emit_xml_document(
            document,
            emitters,
            XmlDocumentStatus {
                sha256_hex: None,
                parse_status: "skipped",
                parse_error: None,
                root_name: Some("skipped".to_string()),
                namespace: None,
                store_raw_xml: false,
            },
        )?;
    }
    Ok(())
}

fn parse_one_document(
    config: &Config,
    document: &DocumentSource,
    emitters: &Emitters,
) -> Result<()> {
    let bytes = match read_document_bytes(document) {
        Ok(bytes) => bytes,
        Err(error) => {
            emit_xml_document(
                document,
                emitters,
                XmlDocumentStatus {
                    sha256_hex: None,
                    parse_status: "failed",
                    parse_error: Some(error.to_string()),
                    root_name: None,
                    namespace: None,
                    store_raw_xml: config.store_raw_xml,
                },
            )?;
            return Ok(());
        }
    };

    let sha256_hex = hex::encode(Sha256::digest(&bytes));
    if config.store_raw_xml {
        emit_raw_xml(document, emitters, &bytes)?;
    }

    match parse_bytes(config, document, &bytes, &sha256_hex) {
        Ok(rows) => {
            emit_xml_document(
                document,
                emitters,
                XmlDocumentStatus {
                    sha256_hex: Some(sha256_hex),
                    parse_status: "parsed",
                    parse_error: None,
                    root_name: rows.0,
                    namespace: rows.1,
                    store_raw_xml: config.store_raw_xml,
                },
            )?;
            rows.2.flush(emitters)?;
        }
        Err(error) => {
            emit_xml_document(
                document,
                emitters,
                XmlDocumentStatus {
                    sha256_hex: Some(sha256_hex),
                    parse_status: "failed",
                    parse_error: Some(error.to_string()),
                    root_name: None,
                    namespace: None,
                    store_raw_xml: config.store_raw_xml,
                },
            )?;
        }
    }

    Ok(())
}

fn read_document_bytes(document: &DocumentSource) -> Result<Vec<u8>> {
    if let Some(member_name) = &document.zip_member_name {
        let file = File::open(&document.payload_path)?;
        let mut archive = ZipArchive::new(file)?;
        let mut member = archive.by_name(member_name)?;
        let mut bytes = Vec::with_capacity(member.size().min(32 * 1024 * 1024) as usize);
        member.read_to_end(&mut bytes)?;
        Ok(bytes)
    } else {
        std::fs::read(&document.payload_path).map_err(Into::into)
    }
}

fn emit_xml_document(
    document: &DocumentSource,
    emitters: &Emitters,
    status: XmlDocumentStatus,
) -> Result<()> {
    emitters.send(
        CopyTarget::XmlDocument,
        &[
            opt(document.batch_id),
            opt(document.document_uid),
            opt(document.payload_uid),
            document.zip_member_name.clone(),
            opt(document.operator_directory.clone()),
            opt(document.document_path.clone()),
            document.size_bytes.map(|v| v.to_string()),
            status.sha256_hex,
            opt(status.root_name.as_deref().unwrap_or("unknown")),
            status.namespace,
            None,
            opt(status.parse_status),
            status.parse_error,
            opt(status.store_raw_xml),
        ],
    )
}

fn emit_raw_xml(document: &DocumentSource, emitters: &Emitters, bytes: &[u8]) -> Result<()> {
    let raw_xml_text = std::str::from_utf8(bytes).ok().map(ToOwned::to_owned);
    emitters.send(
        CopyTarget::XmlDocumentRaw,
        &[
            opt(document.batch_id),
            opt(document.document_uid),
            raw_xml_text,
            opt(gzip_hex(bytes)?),
        ],
    )
}

fn gzip_hex(bytes: &[u8]) -> Result<String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(bytes)
        .context("failed to compress raw XML")?;
    let compressed = encoder.finish().context("failed to finish raw XML gzip")?;
    Ok(hex::encode(compressed))
}

fn parse_bytes(
    _config: &Config,
    document: &DocumentSource,
    bytes: &[u8],
    _sha256_hex: &str,
) -> Result<(Option<String>, Option<String>, DocumentRows)> {
    let _arena = Bump::new();
    let mut reader = Reader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut path = Vec::<String>::new();
    let mut object_stack = Vec::<ObjectCtx>::new();
    let mut rows = DocumentRows::new();
    let mut next_ordinal = 1_u32;
    let mut root_name = None;
    let mut root_namespace = None;
    let mut current_frame_uid = None;
    let mut current_frame_currency = None;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(start) => {
                let name = local_name(start.name().as_ref());
                if root_name.is_none() {
                    root_namespace = namespace_from(&start);
                    root_name = Some(name.clone());
                }
                path.push(name.clone());
                let attrs = read_attrs(&reader, &start)?;
                if let Some(netex_id) = attrs.get("id").cloned() {
                    let uid = object_uid(document.document_uid, next_ordinal);
                    let parent_uid = object_stack.last().map(|ctx| ctx.uid);
                    let fare_table_uid = nearest_uid(&object_stack, "FareTable");
                    let sales_offer_package_uid = nearest_uid(&object_stack, "SalesOfferPackage");
                    let parent_axis_uid = nearest_axis_uid(&object_stack);
                    let is_frame = name.ends_with("Frame");
                    let frame_uid = if is_frame {
                        Some(uid)
                    } else {
                        current_frame_uid
                    };
                    if is_frame {
                        current_frame_uid = Some(uid);
                    }
                    let ctx = ObjectCtx {
                        uid,
                        ordinal: next_ordinal,
                        element: name.clone(),
                        netex_id: netex_id.clone(),
                        version: attrs.get("version").cloned(),
                        parent_uid,
                        frame_uid,
                        fare_table_uid,
                        sales_offer_package_uid,
                        parent_axis_uid,
                        frame_currency: current_frame_currency.clone(),
                        path: path.join("/"),
                        attrs,
                        fields: HashMap::new(),
                        lists: HashMap::new(),
                    };
                    rows.object_index
                        .insert(object_key(&name, &netex_id, ctx.version.as_deref()), uid);
                    rows.object_index
                        .insert(object_key("", &netex_id, ctx.version.as_deref()), uid);
                    next_ordinal += 1;
                    object_stack.push(ctx);
                }
            }
            Event::Empty(start) => {
                let name = local_name(start.name().as_ref());
                let attrs = read_attrs(&reader, &start)?;
                if name.ends_with("Ref") {
                    handle_ref(document, &mut rows, &mut object_stack, &path, &name, &attrs);
                } else if let Some(netex_id) = attrs.get("id").cloned() {
                    path.push(name.clone());
                    let uid = object_uid(document.document_uid, next_ordinal);
                    let parent_uid = object_stack.last().map(|ctx| ctx.uid);
                    let ctx = ObjectCtx {
                        uid,
                        ordinal: next_ordinal,
                        element: name.clone(),
                        netex_id: netex_id.clone(),
                        version: attrs.get("version").cloned(),
                        parent_uid,
                        frame_uid: current_frame_uid,
                        fare_table_uid: nearest_uid(&object_stack, "FareTable"),
                        sales_offer_package_uid: nearest_uid(&object_stack, "SalesOfferPackage"),
                        parent_axis_uid: nearest_axis_uid(&object_stack),
                        frame_currency: current_frame_currency.clone(),
                        path: path.join("/"),
                        attrs,
                        fields: HashMap::new(),
                        lists: HashMap::new(),
                    };
                    rows.object_index
                        .insert(object_key(&name, &netex_id, ctx.version.as_deref()), uid);
                    rows.object_index
                        .insert(object_key("", &netex_id, ctx.version.as_deref()), uid);
                    emit_object(document, &mut rows, ctx)?;
                    next_ordinal += 1;
                    path.pop();
                }
            }
            Event::Text(text) => {
                let value = String::from_utf8_lossy(text.as_ref()).trim().to_string();
                if !value.is_empty() {
                    if let Some(element) = path.last().cloned() {
                        if element.ends_with("Ref") {
                            handle_text_ref(
                                document,
                                &mut rows,
                                &mut object_stack,
                                &element,
                                &value,
                            );
                        } else if let Some(ctx) = object_stack.last_mut() {
                            capture_text(ctx, &element, &value);
                            if ctx.element.ends_with("Frame") && element == "DefaultCurrency" {
                                current_frame_currency = Some(value);
                            }
                        }
                    }
                }
            }
            Event::End(end) => {
                let name = local_name(end.name().as_ref());
                if object_stack
                    .last()
                    .map(|ctx| ctx.element.as_str() == name.as_str())
                    .unwrap_or(false)
                {
                    let ctx = object_stack.pop().expect("checked above");
                    if ctx.element.ends_with("Frame") {
                        current_frame_uid = object_stack
                            .iter()
                            .rev()
                            .find(|item| item.element.ends_with("Frame"))
                            .map(|item| item.uid);
                        current_frame_currency = object_stack
                            .iter()
                            .rev()
                            .find(|item| item.element.ends_with("Frame"))
                            .and_then(|item| item.fields.get("DefaultCurrency").cloned());
                    }
                    emit_object(document, &mut rows, ctx)?;
                }
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok((root_name, root_namespace, rows))
}

fn emit_object(document: &DocumentSource, rows: &mut DocumentRows, ctx: ObjectCtx) -> Result<()> {
    let attrs_json = serde_json::to_string(&ctx.attrs)?;
    rows.push(
        CopyTarget::NetexObject,
        vec![
            opt(document.batch_id),
            opt(document.document_uid),
            opt(ctx.uid),
            opt(ctx.ordinal),
            ctx.parent_uid.map(|v| v.to_string()),
            opt(ctx.element.clone()),
            opt(ctx.netex_id.clone()),
            ctx.version.clone(),
            ctx.attrs.get("codespace").cloned(),
            None,
            ctx.attrs.get("type").cloned(),
            ctx.frame_uid.map(|v| v.to_string()),
            ctx.field("TypeOfFrameRef"),
            ctx.field("Name"),
            ctx.field("Description"),
            ctx.field("PrivateCode"),
            ctx.field("PublicCode"),
            opt(ctx.path.clone()),
            opt(attrs_json),
        ],
    );

    if ctx.element.ends_with("Frame") {
        rows.push(
            CopyTarget::Frame,
            vec![
                opt(document.batch_id),
                opt(ctx.uid),
                opt(ctx.element.clone()),
                ctx.field("TypeOfFrameRef"),
                ctx.field("DefaultCurrency"),
                ctx.field("DefaultLanguage"),
            ],
        );
    }

    if PRICEABLE_ELEMENTS.contains(&ctx.element.as_str())
        || FARE_PRODUCT_ELEMENTS.contains(&ctx.element.as_str())
    {
        rows.push(
            CopyTarget::PriceableObject,
            vec![
                opt(document.batch_id),
                opt(ctx.uid),
                opt(ctx.element.clone()),
                ctx.field("PricingRuleRef"),
                ctx.field("RoundingRef"),
                ctx.field("PriceUnitRef"),
                opt("{}"),
            ],
        );
    }

    match ctx.element.as_str() {
        "Operator" | "GroupOfOperators" => emit_operator(document, rows, &ctx),
        "Line" => emit_line(document, rows, &ctx),
        "ScheduledStopPoint" | "FareScheduledStopPoint" => emit_stop(document, rows, &ctx),
        "FareZone" => emit_fare_zone(document, rows, &ctx),
        "Tariff" => emit_tariff(document, rows, &ctx),
        element if FARE_PRODUCT_ELEMENTS.contains(&element) => {
            emit_fare_product(document, rows, &ctx)
        }
        "SalesOfferPackage" => emit_sales_offer_package(document, rows, &ctx),
        "SalesOfferPackageElement" => emit_sales_offer_package_element(document, rows, &ctx),
        "DistributionAssignment" => emit_distribution_assignment(document, rows, &ctx),
        "UserProfile"
        | "CompanionProfile"
        | "CommercialProfile"
        | "GroupTicket"
        | "FrequencyOfUse"
        | "UsageValidityPeriod"
        | "RoundTrip"
        | "Transferability" => emit_usage_parameter(document, rows, &ctx),
        "FareTable" => emit_fare_table(document, rows, &ctx),
        "FareTableRow" | "FareTableColumn" => emit_fare_table_axis(document, rows, &ctx),
        "DistanceMatrixElement" => emit_distance_matrix_element(document, rows, &ctx),
        element if FARE_PRICE_ELEMENTS.contains(&element) => emit_fare_price(document, rows, &ctx),
        "Cell" => emit_cell(document, rows, &ctx),
        "GenericParameterAssignment" | "AccessRightParameterAssignment" => {
            emit_assignment(document, rows, &ctx)
        }
        "AvailabilityCondition" => emit_availability_condition(document, rows, &ctx),
        "Notice" => emit_notice(document, rows, &ctx),
        "NoticeAssignment" | "SalesNoticeAssignment" => {
            emit_notice_assignment(document, rows, &ctx)
        }
        _ => {}
    }

    Ok(())
}

fn emit_operator(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::Operator,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            opt(if ctx.element == "GroupOfOperators" {
                "group"
            } else {
                "operator"
            }),
            ctx.field("PublicCode")
                .or_else(|| ctx.field("Noc"))
                .or_else(|| atco_like_id(&ctx.netex_id)),
            ctx.field("PublicCode"),
            ctx.field("ShortName").or_else(|| ctx.field("Name")),
            ctx.field("TradingName"),
            ctx.field("PrimaryMode"),
        ],
    );
}

fn emit_line(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::Line,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("OperatorRef"),
            ctx.field("PublicCode"),
            ctx.field("PrivateCode"),
            ctx.field("LineType"),
            ctx.field("TransportMode"),
        ],
    );
}

fn emit_stop(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::ScheduledStopPoint,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            derive_atco(&ctx.netex_id),
            ctx.field("Name"),
            ctx.field("TopographicPlaceRef"),
            ctx.field("TopographicProjectionRef"),
            None,
        ],
    );
}

fn emit_fare_zone(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::FareZone,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            opt(ctx.netex_id.clone()),
            ctx.field("ParentFareZoneRef"),
            ctx.field("ScopingMethod"),
            ctx.field("ZoneTopology"),
            ctx.field("TypeOfZoneRef"),
            ctx.field("TransportOrganisationRef"),
            ctx.list("GroupOfOperatorsRef"),
            ctx.list("ContainsFareZoneRef"),
            ctx.list("NeighbourFareZoneRef"),
        ],
    );
}

fn emit_tariff(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::Tariff,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("OperatorRef"),
            ctx.field("LineRef"),
            ctx.field("GroupOfLinesRef"),
            ctx.field("GroupOfOperatorsRef"),
            ctx.field("TypeOfTariffRef"),
            ctx.field("TariffBasis"),
            ctx.bool_field("ReturnFareTwiceSingle"),
            ctx.field("PriceUnitRef"),
            ctx.field("FromDate").or_else(|| ctx.field("ValidFrom")),
            ctx.field("ToDate").or_else(|| ctx.field("ValidTo")),
        ],
    );
}

fn emit_fare_product(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::FareProduct,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            opt(ctx.element.clone()),
            ctx.field("OperatorRef"),
            ctx.field("TypeOfFareProductRef"),
            ctx.field("ProductType"),
            ctx.field("ChargingMomentRef"),
            ctx.field("ChargingMomentType"),
            ctx.field("PriceUnitRef"),
            ctx.field("Amount"),
            opt(ctx.element.clone()),
            ctx.field("DiscountBasis"),
            opt("{}"),
        ],
    );
}

fn emit_sales_offer_package(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::SalesOfferPackage,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("TypeOfSalesOfferPackageRef"),
            ctx.list("GroupOfSalesOfferPackagesRef"),
            ctx.field("RoundingRef"),
            ctx.field("FromDate"),
            ctx.field("ToDate"),
            opt("{}"),
            opt("{}"),
        ],
    );
}

fn emit_sales_offer_package_element(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    ctx: &ObjectCtx,
) {
    rows.push(
        CopyTarget::SalesOfferPackageElement,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.sales_offer_package_uid.map(|v| v.to_string()),
            ctx.field("FareProductRef"),
            ctx.resolved_field("FareProductRef", rows),
            ctx.field("PreassignedFareProductRef"),
            ctx.field("AmountOfPriceUnitProductRef"),
            ctx.field("TypeOfTravelDocumentRef"),
            ctx.bool_field("RequiresValidation"),
            ctx.attrs.get("order").cloned(),
            opt("{}"),
        ],
    );
}

fn emit_distribution_assignment(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    ctx: &ObjectCtx,
) {
    rows.push(
        CopyTarget::DistributionAssignment,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.sales_offer_package_uid.map(|v| v.to_string()),
            ctx.field("DistributionChannelRef"),
            ctx.field("DistributionChannelType"),
            ctx.field("FulfilmentMethodRef"),
            ctx.list("PaymentMethods"),
            ctx.list("TicketingServiceFacilityList"),
            ctx.attrs.get("order").cloned(),
            opt("{}"),
        ],
    );
}

fn emit_usage_parameter(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::UsageParameter,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            opt(ctx.element.clone()),
            ctx.field("UserType"),
            ctx.field("MinimumAge"),
            ctx.field("MaximumAge"),
            ctx.field("DiscountBasis"),
            ctx.field("ProofRequired"),
            ctx.field("FrequencyType"),
            ctx.field("TripType"),
            ctx.bool_field("CanTransfer"),
            ctx.field("StartTime"),
            ctx.field("EndTime"),
            ctx.field("Duration"),
            opt("{}"),
        ],
    );
}

fn emit_fare_table(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::FareTable,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("TypeOfFareTableRef"),
            ctx.field("StartDate").or_else(|| ctx.field("FromDate")),
            ctx.field("EndDate").or_else(|| ctx.field("ToDate")),
            ctx.field("EmbargoUntil"),
            ctx.field("TariffRef"),
            ctx.resolved_field("TariffRef", rows),
            ctx.field("LineRef"),
            ctx.resolved_field("LineRef", rows),
            ctx.field("FareProductRef")
                .or_else(|| ctx.field("PreassignedFareProductRef")),
            ctx.resolved_field("FareProductRef", rows)
                .or_else(|| ctx.resolved_field("PreassignedFareProductRef", rows)),
            ctx.field("SalesOfferPackageRef"),
            ctx.resolved_field("SalesOfferPackageRef", rows),
            ctx.field("UserProfileRef"),
            ctx.field("TypeOfTravelDocumentRef"),
            ctx.list("PricesForRef"),
            None,
            ctx.list("LimitationRef"),
            opt("{}"),
        ],
    );
}

fn emit_fare_table_axis(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::FareTableAxis,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.fare_table_uid
                .map(|v| v.to_string())
                .or(ctx.parent_uid.map(|v| v.to_string())),
            opt(if ctx.element == "FareTableColumn" {
                "column"
            } else {
                "row"
            }),
            ctx.attrs.get("order").cloned(),
            ctx.parent_axis_uid.map(|v| v.to_string()),
            ctx.field("FareZoneRef")
                .or_else(|| ctx.field("TariffZoneRef")),
            ctx.resolved_field("FareZoneRef", rows)
                .or_else(|| ctx.resolved_field("TariffZoneRef", rows)),
            ctx.field("UserProfileRef"),
            ctx.field("TimeIntervalRef"),
            ctx.field("TypeOfTravelDocumentRef"),
            ctx.list("RepresentingRef"),
            None,
        ],
    );
}

fn emit_distance_matrix_element(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    ctx: &ObjectCtx,
) {
    rows.push(
        CopyTarget::DistanceMatrixElement,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("StartStopPointRef"),
            ctx.resolved_field("StartStopPointRef", rows),
            ctx.field("StartTariffZoneRef")
                .or_else(|| ctx.field("StartFareZoneRef")),
            ctx.resolved_field("StartTariffZoneRef", rows)
                .or_else(|| ctx.resolved_field("StartFareZoneRef", rows)),
            ctx.field("FromFareSectionRef"),
            ctx.field("FromFarePointInPatternRef"),
            ctx.field("EndStopPointRef"),
            ctx.resolved_field("EndStopPointRef", rows),
            ctx.field("EndTariffZoneRef")
                .or_else(|| ctx.field("EndFareZoneRef")),
            ctx.resolved_field("EndTariffZoneRef", rows)
                .or_else(|| ctx.resolved_field("EndFareZoneRef", rows)),
            ctx.field("ToFareSectionRef"),
            ctx.field("ToFarePointInPatternRef"),
            ctx.list("SeriesConstraintRef"),
            ctx.list("GroupOfDistanceMatrixElementsRef"),
            ctx.field("Name"),
        ],
    );
}

fn emit_fare_price(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    let currency = ctx.field("Currency").or_else(|| ctx.frame_currency.clone());
    let currency_source = if ctx.field("Currency").is_some() {
        Some("xml".to_string())
    } else if ctx.frame_currency.is_some() {
        Some("frame_default".to_string())
    } else {
        Some("unknown".to_string())
    };
    rows.push(
        CopyTarget::FarePrice,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.parent_uid.map(|v| v.to_string()),
            ctx.fare_table_uid.map(|v| v.to_string()),
            opt(ctx.element.clone()),
            ctx.field("Amount"),
            currency.clone(),
            currency_source.clone(),
            ctx.field("Units"),
            ctx.field("PriceUnitRef"),
            ctx.field("PricingRuleRef"),
            ctx.field("RoundingRef"),
            ctx.field("DerivedFromPriceRef"),
            ctx.bool_field("IsAllowed")
                .or_else(|| ctx.bool_field("Allowed")),
            ctx.field("PricedObjectRef")
                .or_else(|| ctx.field("DistanceMatrixElementRef")),
            ctx.resolved_field("PricedObjectRef", rows)
                .or_else(|| ctx.resolved_field("DistanceMatrixElementRef", rows)),
        ],
    );

    if ctx.element == "DistanceMatrixElementPrice" {
        rows.push(
            CopyTarget::DistanceMatrixElementPrice,
            vec![
                opt(document.batch_id),
                opt(ctx.uid),
                ctx.fare_table_uid.map(|v| v.to_string()),
                ctx.field("DistanceMatrixElementRef"),
                ctx.resolved_field("DistanceMatrixElementRef", rows),
                ctx.field("GeographicalIntervalPriceRef"),
                ctx.resolved_field("GeographicalIntervalPriceRef", rows),
                opt(ctx.uid),
                ctx.field("Amount"),
                ctx.bool_field("IsAllowed")
                    .or_else(|| ctx.bool_field("Allowed")),
                currency,
                currency_source,
            ],
        );
    }
}

fn emit_cell(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    let currency = ctx.field("Currency").or_else(|| ctx.frame_currency.clone());
    let currency_source = if ctx.field("Currency").is_some() {
        Some("xml".to_string())
    } else if ctx.frame_currency.is_some() {
        Some("frame_default".to_string())
    } else {
        Some("unknown".to_string())
    };
    rows.push(
        CopyTarget::Cell,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.fare_table_uid.map(|v| v.to_string()),
            ctx.field("RowRef"),
            ctx.resolved_field("RowRef", rows),
            ctx.field("ColumnRef"),
            ctx.resolved_field("ColumnRef", rows),
            ctx.field("FarePriceRef"),
            ctx.resolved_field("FarePriceRef", rows),
            ctx.field("PriceGroupRef"),
            ctx.resolved_field("PriceGroupRef", rows),
            ctx.field("DistanceMatrixElementPriceRef"),
            ctx.resolved_field("DistanceMatrixElementPriceRef", rows),
            ctx.list("PricesForRef"),
            None,
            ctx.field("GroupOfDistanceMatrixElementsRef"),
            ctx.field("GroupOfLinesRef"),
            ctx.field("LineRef"),
            ctx.field("FareSectionRef"),
            ctx.field("Amount"),
            currency,
            currency_source,
            ctx.bool_field("IsAllowed")
                .or_else(|| ctx.bool_field("Allowed")),
        ],
    );
}

fn emit_assignment(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::AccessRightParameterAssignment,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.parent_uid.map(|v| v.to_string()),
            ctx.field("AssignmentType"),
            ctx.field("ValidityParameterAssignmentType"),
            ctx.field("LimitationGroupingType"),
            ctx.field("ValidityParameterGroupingType"),
            opt("{}"),
        ],
    );

    let mut order = 0;
    for (key, values) in &ctx.lists {
        for value in values {
            rows.push(
                CopyTarget::AccessRightParameterAssignmentItem,
                vec![
                    opt(document.batch_id),
                    opt(ctx.uid),
                    opt("include"),
                    opt(parameter_kind(key)),
                    opt(value.clone()),
                    None,
                    None,
                    None,
                    opt(order),
                ],
            );
            order += 1;
        }
    }
}

fn emit_availability_condition(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    ctx: &ObjectCtx,
) {
    rows.push(
        CopyTarget::AvailabilityCondition,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("FromDate"),
            ctx.field("ToDate"),
            ctx.field("DayTypeRef"),
            ctx.field("StartTime"),
            ctx.field("EndTime"),
            ctx.list("DaysOfWeek"),
            opt("{}"),
        ],
    );
}

fn emit_notice(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::Notice,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.field("PublicCode").or_else(|| ctx.field("NoticeCode")),
            ctx.field("Text").or_else(|| ctx.field("Description")),
            ctx.field("PrivateCode"),
            opt("{}"),
        ],
    );
}

fn emit_notice_assignment(document: &DocumentSource, rows: &mut DocumentRows, ctx: &ObjectCtx) {
    rows.push(
        CopyTarget::NoticeAssignment,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            ctx.parent_uid.map(|v| v.to_string()),
            ctx.field("NoticeRef"),
            ctx.resolved_field("NoticeRef", rows),
            opt("{}"),
        ],
    );
}

fn handle_ref(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    object_stack: &mut [ObjectCtx],
    path: &[String],
    ref_name: &str,
    attrs: &HashMap<String, String>,
) {
    let Some(target_ref) = attrs.get("ref").cloned() else {
        return;
    };
    let target_version = attrs.get("version").cloned();
    let source_object_id = object_stack.last().map(|ctx| ctx.uid);
    let source_element_name = object_stack.last().map(|ctx| ctx.element.clone());
    let target_element = target_element_name(ref_name);
    let resolved = target_element
        .as_deref()
        .and_then(|element| {
            rows.object_index
                .get(&object_key(element, &target_ref, target_version.as_deref()))
                .copied()
        })
        .or_else(|| {
            rows.object_index
                .get(&object_key("", &target_ref, target_version.as_deref()))
                .copied()
        });

    rows.references.push(ReferenceRow {
        batch_id: document.batch_id,
        document_uid: document.document_uid,
        source_object_id,
        source_element_name,
        ref_element_name: ref_name.to_string(),
        target_element_name: target_element.clone(),
        target_ref: target_ref.clone(),
        target_version,
        text_value: None,
        resolved_object_id: resolved,
    });

    if let Some(ctx) = object_stack.last_mut() {
        capture_ref_field(ctx, path, ref_name, &target_ref, resolved);
        maybe_emit_fare_zone_member(document, rows, ctx, path, ref_name, &target_ref, resolved);
    }
}

fn handle_text_ref(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    object_stack: &mut [ObjectCtx],
    ref_name: &str,
    value: &str,
) {
    let source = object_stack.last();
    rows.references.push(ReferenceRow {
        batch_id: document.batch_id,
        document_uid: document.document_uid,
        source_object_id: source.map(|ctx| ctx.uid),
        source_element_name: source.map(|ctx| ctx.element.clone()),
        ref_element_name: ref_name.to_string(),
        target_element_name: target_element_name(ref_name),
        target_ref: value.to_string(),
        target_version: None,
        text_value: Some(value.to_string()),
        resolved_object_id: None,
    });
}

fn maybe_emit_fare_zone_member(
    document: &DocumentSource,
    rows: &mut DocumentRows,
    ctx: &ObjectCtx,
    path: &[String],
    ref_name: &str,
    target_ref: &str,
    resolved: Option<u64>,
) {
    if ctx.element != "FareZone" {
        return;
    }
    let path_has_members = path.iter().any(|item| {
        matches!(
            item.as_str(),
            "members" | "fareSections" | "points" | "projections"
        )
    });
    let member_kind = match ref_name {
        "ScheduledStopPointRef" | "FareScheduledStopPointRef" if path_has_members => {
            "scheduled_stop_point"
        }
        "TopographicProjectionRef" if path_has_members => "topographic_projection",
        "FareSectionRef" if path_has_members || path.iter().any(|item| item == "fareSections") => {
            "fare_section"
        }
        "FarePointInPatternRef" if path_has_members => "fare_point_in_pattern",
        _ => return,
    };
    rows.push(
        CopyTarget::FareZoneMember,
        vec![
            opt(document.batch_id),
            opt(ctx.uid),
            opt(target_ref),
            opt(member_kind),
            if member_kind == "scheduled_stop_point" {
                resolved.map(|v| v.to_string())
            } else {
                None
            },
            if member_kind == "fare_section" {
                resolved.map(|v| v.to_string())
            } else {
                None
            },
            if member_kind == "fare_point_in_pattern" {
                resolved.map(|v| v.to_string())
            } else {
                None
            },
            None,
        ],
    );
}

fn capture_text(ctx: &mut ObjectCtx, element: &str, value: &str) {
    let normalized = normalize_text_element(element);
    if matches!(
        normalized.as_str(),
        "PaymentMethods" | "DaysOfWeek" | "TicketingServiceFacilityList"
    ) {
        ctx.lists
            .entry(normalized)
            .or_default()
            .extend(value.split_whitespace().map(ToString::to_string));
    } else {
        ctx.fields
            .entry(normalized)
            .or_insert_with(|| value.to_string());
    }
}

fn capture_ref_field(
    ctx: &mut ObjectCtx,
    path: &[String],
    ref_name: &str,
    target_ref: &str,
    _resolved: Option<u64>,
) {
    let key = normalize_ref_name(ref_name, path);
    if is_list_ref(&key, path) {
        ctx.lists
            .entry(key)
            .or_default()
            .push(target_ref.to_string());
    } else {
        ctx.fields
            .entry(key)
            .or_insert_with(|| target_ref.to_string());
    }
}

fn read_attrs(
    reader: &Reader<Cursor<&[u8]>>,
    start: &BytesStart<'_>,
) -> Result<HashMap<String, String>> {
    let mut attrs = HashMap::new();
    for attr in start.attributes().with_checks(false) {
        let attr = attr?;
        let key = local_name(attr.key.as_ref());
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(attr.value.as_ref()).to_string());
        attrs.insert(key, value);
    }
    Ok(attrs)
}

fn namespace_from(start: &BytesStart<'_>) -> Option<String> {
    for attr in start.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == b"xmlns" {
            return Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
        }
    }
    None
}

fn local_name(name: &[u8]) -> String {
    let full = String::from_utf8_lossy(name);
    full.rsplit(':').next().unwrap_or(&full).to_string()
}

fn object_uid(document_uid: u64, ordinal: u32) -> u64 {
    (document_uid << 32) | u64::from(ordinal)
}

fn object_key(element: &str, netex_id: &str, version: Option<&str>) -> String {
    format!("{element}\u{1f}{netex_id}\u{1f}{}", version.unwrap_or(""))
}

fn nearest_uid(stack: &[ObjectCtx], element: &str) -> Option<u64> {
    stack
        .iter()
        .rev()
        .find(|ctx| ctx.element == element)
        .map(|ctx| ctx.uid)
}

fn nearest_axis_uid(stack: &[ObjectCtx]) -> Option<u64> {
    stack
        .iter()
        .rev()
        .find(|ctx| matches!(ctx.element.as_str(), "FareTableRow" | "FareTableColumn"))
        .map(|ctx| ctx.uid)
}

fn normalize_text_element(element: &str) -> String {
    match element {
        "Label" => "Description",
        "ShortName" => "ShortName",
        "Name" | "Description" | "PrivateCode" | "PublicCode" | "Text" | "DefaultCurrency"
        | "DefaultLanguage" => element,
        "Amount" | "Currency" | "Units" | "IsAllowed" | "Allowed" => element,
        "TariffBasis" | "ProductType" | "UserType" | "MinimumAge" | "MaximumAge"
        | "DiscountBasis" => element,
        "ScopingMethod" | "ZoneTopology" | "TransportMode" | "LineType" | "PrimaryMode" => element,
        "StartDate" | "EndDate" | "FromDate" | "ToDate" | "StartTime" | "EndTime" | "Duration" => {
            element
        }
        "PaymentMethods" | "DaysOfWeek" | "TicketingServiceFacilityList" => element,
        _ => element,
    }
    .to_string()
}

fn normalize_ref_name(ref_name: &str, path: &[String]) -> String {
    match ref_name {
        "FareScheduledStopPointRef" => "ScheduledStopPointRef",
        "StartTariffZoneRef" => "StartTariffZoneRef",
        "EndTariffZoneRef" => "EndTariffZoneRef",
        "FareZoneRef" if path.iter().any(|item| item == "contains") => "ContainsFareZoneRef",
        "FareZoneRef" if path.iter().any(|item| item == "neighbours") => "NeighbourFareZoneRef",
        "VersionOfObjectRef" if path.iter().any(|item| item == "pricesFor") => "PricesForRef",
        "VersionOfObjectRef" if path.iter().any(|item| item == "representing") => "RepresentingRef",
        _ => ref_name,
    }
    .to_string()
}

fn is_list_ref(key: &str, path: &[String]) -> bool {
    key.ends_with("Refs")
        || matches!(
            key,
            "GroupOfOperatorsRef"
                | "GroupOfSalesOfferPackagesRef"
                | "GroupOfDistanceMatrixElementsRef"
                | "SeriesConstraintRef"
                | "PricesForRef"
                | "RepresentingRef"
                | "LimitationRef"
                | "ContainsFareZoneRef"
                | "NeighbourFareZoneRef"
        )
        || path
            .iter()
            .any(|item| matches!(item.as_str(), "pricesFor" | "limitations" | "representing"))
}

fn target_element_name(ref_name: &str) -> Option<String> {
    let stripped = ref_name.strip_suffix("Ref")?;
    let normalized = stripped
        .strip_prefix("Start")
        .or_else(|| stripped.strip_prefix("End"))
        .or_else(|| stripped.strip_prefix("From"))
        .or_else(|| stripped.strip_prefix("To"))
        .unwrap_or(stripped);
    Some(
        match normalized {
            "TariffZone" => "FareZone",
            "FareScheduledStopPoint" => "ScheduledStopPoint",
            "VersionOfObject" => "",
            other => other,
        }
        .to_string(),
    )
}

fn derive_atco(netex_id: &str) -> Option<String> {
    netex_id
        .strip_prefix("atco:")
        .or_else(|| netex_id.strip_prefix("naptStop:"))
        .map(ToString::to_string)
}

fn atco_like_id(netex_id: &str) -> Option<String> {
    netex_id
        .split(':')
        .next_back()
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

fn parameter_kind(key: &str) -> &'static str {
    match key {
        "LineRef" => "line",
        "GroupOfLinesRef" => "group_of_lines",
        "OperatorRef" => "operator",
        "FareZoneRef" => "fare_zone",
        "TariffRef" => "tariff",
        "FareProductRef" | "PreassignedFareProductRef" => "fare_product",
        "SalesOfferPackageRef" => "sales_offer_package",
        "DistributionChannelRef" => "distribution_channel",
        "UsageParameterRef" => "usage_parameter",
        _ => "other",
    }
}

impl ObjectCtx {
    fn field(&self, key: &str) -> Option<String> {
        self.fields.get(key).cloned()
    }

    fn bool_field(&self, key: &str) -> Option<String> {
        self.field(key)
            .map(|value| match value.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => "true".to_string(),
                "false" | "0" | "no" => "false".to_string(),
                _ => value,
            })
    }

    fn list(&self, key: &str) -> Option<String> {
        self.lists.get(key).and_then(|values| join_list(values))
    }

    fn resolved_field(&self, key: &str, rows: &DocumentRows) -> Option<String> {
        let target_ref = self.field(key)?;
        let target_element = target_element_name(key);
        target_element
            .as_deref()
            .and_then(|element| {
                rows.object_index
                    .get(&object_key(element, &target_ref, None))
                    .copied()
            })
            .or_else(|| {
                rows.object_index
                    .get(&object_key("", &target_ref, None))
                    .copied()
            })
            .map(|uid| uid.to_string())
    }
}
