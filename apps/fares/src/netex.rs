use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node, NodeId, ParsingOptions};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

const NS: &str = "http://www.netex.org.uk/netex";

#[derive(Debug, Serialize)]
pub struct Entity {
    pub identity: String,
    pub category: String,
    pub kind: String,
    pub reference: String,
    pub version: String,
    pub occurrence: u32,
    pub name: Option<String>,
    pub amount: Option<String>,
    pub currency: Option<String>,
    pub product_ref: Option<String>,
    pub profile_ref: Option<String>,
    pub package_ref: Option<String>,
    pub matrix_ref: Option<String>,
    pub price_ref: Option<String>,
    pub group_ref: Option<String>,
    pub start_ref: Option<String>,
    pub end_ref: Option<String>,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Link {
    pub kind: String,
    pub code: String,
    pub reference: String,
    pub qualifier: String,
    pub name: Option<String>,
}

pub struct Parsed {
    pub timestamp: Option<String>,
    pub currency: Option<String>,
    pub entities: Vec<Entity>,
    pub links: Vec<Link>,
}

fn is(node: Node<'_, '_>, tag: &str) -> bool {
    node.has_tag_name((NS, tag))
}
fn child<'a, 'b>(node: Node<'a, 'b>, tag: &str) -> Option<Node<'a, 'b>> {
    node.children().find(|n| is(*n, tag))
}
fn text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    child(node, tag)
        .and_then(|n| n.text())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
fn reference(node: Node<'_, '_>, tag: &str) -> Option<String> {
    node.descendants()
        .find(|n| is(*n, tag))
        .and_then(|n| n.attribute("ref"))
        .map(str::to_owned)
}
fn context_reference(node: Node<'_, '_>, tag: &str) -> Option<String> {
    // A table's pricesFor applies to its cells. Do not inherit another nested
    // table's pricesFor or infer a product merely because it shares a file.
    node.ancestors()
        .filter(|n| is(*n, "FareTable"))
        .find_map(|table| child(table, "pricesFor").and_then(|n| reference(n, tag)))
}
fn currency(node: Node<'_, '_>, defaults: &HashMap<NodeId, String>) -> Option<String> {
    text(node, "Currency")
        .or_else(|| node.attribute("currency").map(str::to_owned))
        .or_else(|| {
            node.ancestors()
                .find_map(|n| defaults.get(&n.id()).cloned())
        })
}

// The JSON tree retains names, attributes, child order and meaningful text.
// Large nested tables/matrices are stored as separate rows; source XML retains
// the full original tree, namespaces, comments and unprojected conditions.
fn tree(node: Node<'_, '_>, summary: bool) -> Value {
    let attrs: BTreeMap<_, _> = node
        .attributes()
        .map(|a| {
            let name = a
                .namespace()
                .map(|ns| format!("{{{ns}}}{}", a.name()))
                .unwrap_or_else(|| a.name().into());
            (name, a.value())
        })
        .collect();
    let children: Vec<_> = node
        .children()
        .filter_map(|n| {
            if n.is_element() {
                if summary
                    && matches!(
                        n.tag_name().name(),
                        "columns"
                            | "rows"
                            | "cells"
                            | "fareTables"
                            | "distanceMatrixElements"
                            | "fareStructureElements"
                    )
                {
                    None
                } else {
                    Some(tree(n, summary))
                }
            } else if n.is_text() && !n.text().unwrap_or("").trim().is_empty() {
                Some(json!(n.text().unwrap().trim()))
            } else {
                None
            }
        })
        .collect();
    json!({"name":node.tag_name().name(),"namespace":node.tag_name().namespace(),"attributes":attrs,"children":children})
}

fn category(kind: &str) -> Option<&'static str> {
    match kind {
        "PreassignedFareProduct"
        | "AmountOfPriceUnitProduct"
        | "SalesDiscountRight"
        | "CappedDiscountRight"
        | "SupplementProduct"
        | "FareProduct" => Some("product"),
        "FareZone" | "TariffZone" => Some("zone"),
        "DistanceMatrixElement" => Some("matrix"),
        "UserProfile" | "GroupTicket" | "CompanionProfile" => Some("profile"),
        "SalesOfferPackage" => Some("offer"),
        "FareTable" => Some("table"),
        "Tariff"
        | "FareStructureElement"
        | "GeographicalInterval"
        | "TimeInterval"
        | "RoundTrip"
        | "FrequencyOfUse"
        | "ValidityCondition"
        | "AvailabilityCondition"
        | "ValidableElement"
        | "GenericParameterAssignment" => Some("rule"),
        "PriceGroup" => Some("price_group"),
        _ if kind.ends_with("Price") => Some("price"),
        _ => None,
    }
}

pub fn parse(xml: &[u8]) -> Result<Parsed> {
    let input = std::str::from_utf8(xml).context("NeTEx must be UTF-8")?;
    let doc = Document::parse_with_options(
        input,
        ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .context("parse NeTEx XML")?;
    ensure!(
        is(doc.root_element(), "PublicationDelivery"),
        "expected a NeTEx PublicationDelivery"
    );
    // Check depth before recursively serializing any selected subtree.
    for node in doc.descendants().filter(|n| n.is_element()) {
        ensure!(
            node.ancestors().take(130).count() <= 129,
            "XML nesting exceeds 128"
        );
    }
    // Resolve defaults once. Searching every ancestor's children per price
    // would be quadratic for large matrices and fare tables.
    let mut defaults = HashMap::new();
    for node in doc.descendants().filter(|n| is(*n, "FrameDefaults")) {
        if let Some(parent) = node.parent()
            && let Some(value) = text(node, "DefaultCurrency")
        {
            defaults.entry(parent.id()).or_insert(value);
        }
    }
    let mut parsed = Parsed {
        timestamp: text(doc.root_element(), "PublicationTimestamp"),
        currency: doc
            .descendants()
            .find(|n| is(*n, "DefaultCurrency"))
            .and_then(|n| n.text())
            .map(|s| s.trim().into()),
        entities: Vec::new(),
        links: Vec::new(),
    };
    let mut occurrences = HashMap::<(String, String, String), u32>::new();
    for node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(NS))
    {
        let kind = node.tag_name().name();
        let id = node.attribute("id");
        if let Some(id) = id {
            if kind == "Operator" {
                let noc =
                    text(node, "PublicCode").or_else(|| id.strip_prefix("noc:").map(str::to_owned));
                if let Some(code) = noc {
                    parsed.links.push(Link {
                        kind: "operator".into(),
                        code,
                        reference: id.into(),
                        qualifier: String::new(),
                        name: text(node, "Name"),
                    });
                }
            }
            if kind == "Line"
                && let Some(code) = text(node, "PublicCode")
            {
                parsed.links.push(Link {
                    kind: "line".into(),
                    code,
                    reference: id.into(),
                    qualifier: reference(node, "OperatorRef").unwrap_or_default(),
                    name: text(node, "Name"),
                });
            }
            if kind == "ScheduledStopPoint" || kind == "StopPlace" {
                let code = stop_code(id).or_else(|| text(node, "PublicCode"));
                if let Some(code) = code {
                    parsed.links.push(Link {
                        kind: "stop".into(),
                        code,
                        reference: id.into(),
                        qualifier: String::new(),
                        name: text(node, "Name"),
                    });
                }
            }
            if kind == "FareZone" || kind == "TariffZone" {
                for stop in node
                    .descendants()
                    .filter(|n| is(*n, "ScheduledStopPointRef") || is(*n, "StopPlaceRef"))
                {
                    if let Some(r) = stop.attribute("ref") {
                        parsed.links.push(Link {
                            kind: "zone_stop".into(),
                            code: stop_code(r).unwrap_or_else(|| r.into()),
                            reference: id.into(),
                            qualifier: r.into(),
                            name: None,
                        });
                    }
                }
            }
        }
        // Referenced external operators and stops still provide join keys.
        if kind == "OperatorRef"
            && let Some(r) = node.attribute("ref")
            && let Some(noc) = r.strip_prefix("noc:")
        {
            parsed.links.push(Link {
                kind: "operator".into(),
                code: noc.into(),
                reference: r.into(),
                qualifier: String::new(),
                name: None,
            });
        }
        if kind == "ScheduledStopPointRef"
            && let Some(r) = node.attribute("ref")
            && let Some(code) = stop_code(r)
        {
            parsed.links.push(Link {
                kind: "stop".into(),
                code,
                reference: r.into(),
                qualifier: String::new(),
                name: None,
            });
        }
        let Some(category) = category(kind) else {
            continue;
        };
        let Some(id) = id else { continue };
        let version = node.attribute("version").unwrap_or("");
        let occurrence = occurrences
            .entry((kind.into(), id.into(), version.into()))
            .or_default();
        *occurrence += 1;
        let amount = text(node, "Amount");
        if let Some(amount) = &amount {
            ensure!(decimal(amount), "invalid decimal amount {amount:?} in {id}");
        }
        let product_ref = reference(node, "PreassignedFareProductRef")
            .or_else(|| reference(node, "AmountOfPriceUnitProductRef"))
            .or_else(|| context_reference(node, "PreassignedFareProductRef"));
        let price_ref = node
            .descendants()
            .filter(|n| n.is_element())
            .find(|n| n.tag_name().name().ends_with("PriceRef"))
            .and_then(|n| n.attribute("ref"))
            .map(str::to_owned);
        parsed.entities.push(Entity {
            identity: serde_json::to_string(&(kind, id, version, *occurrence))?,
            category: category.into(),
            kind: kind.into(),
            reference: id.into(),
            version: version.into(),
            occurrence: *occurrence,
            name: text(node, "Name"),
            amount,
            currency: currency(node, &defaults),
            product_ref,
            profile_ref: reference(node, "UserProfileRef")
                .or_else(|| context_reference(node, "UserProfileRef")),
            package_ref: reference(node, "SalesOfferPackageRef")
                .or_else(|| context_reference(node, "SalesOfferPackageRef")),
            matrix_ref: reference(node, "DistanceMatrixElementRef"),
            price_ref,
            group_ref: reference(node, "PriceGroupRef").or_else(|| {
                node.ancestors()
                    .find(|n| is(*n, "PriceGroup"))
                    .and_then(|n| n.attribute("id"))
                    .map(str::to_owned)
            }),
            start_ref: reference(node, "StartTariffZoneRef")
                .or_else(|| reference(node, "StartFareZoneRef")),
            end_ref: reference(node, "EndTariffZoneRef")
                .or_else(|| reference(node, "EndFareZoneRef")),
            data: serde_json::to_string(&tree(node, matches!(category, "table" | "rule")))?,
        });
    }
    // Collapse repeated link declarations, preferring a named definition.
    parsed.links.sort_by(|a, b| {
        (&a.kind, &a.code, &a.reference, &a.qualifier)
            .cmp(&(&b.kind, &b.code, &b.reference, &b.qualifier))
            .then_with(|| b.name.cmp(&a.name))
    });
    parsed.links.dedup_by(|a, b| {
        (&a.kind, &a.code, &a.reference, &a.qualifier)
            == (&b.kind, &b.code, &b.reference, &b.qualifier)
    });
    Ok(parsed)
}

pub fn stop_code(reference: &str) -> Option<String> {
    ["atco:", "naptan:", "NaPTAN:"]
        .iter()
        .find_map(|prefix| reference.strip_prefix(prefix))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn decimal(value: &str) -> bool {
    let value = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    parts.next().is_none()
        && (!whole.is_empty() || fraction.is_some_and(|s| !s.is_empty()))
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.is_none_or(|s| s.bytes().all(|b| b.is_ascii_digit()))
}
