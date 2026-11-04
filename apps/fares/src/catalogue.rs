//! Conservative, document-local NeTEx projection. Never resolve unqualified refs
//! across documents. Navigation keys are deliberately separate from ticket keys.
use crate::import::hash;
use anyhow::Result;
use roxmltree::{Document, Node, NodeId};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const VERSION: i64 = 1;
const NS: &str = "http://www.netex.org.uk/netex";
fn tag<'a, 'b>(n: Node<'a, 'b>) -> &'b str {
    n.tag_name().name()
}
fn nodes<'a, 'b>(n: Node<'a, 'b>) -> impl Iterator<Item = Node<'a, 'b>> {
    n.descendants()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(NS))
}
fn children<'a, 'b>(n: Node<'a, 'b>) -> impl Iterator<Item = Node<'a, 'b>> {
    n.children()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(NS))
}
fn text(n: Node<'_, '_>, name: &str) -> Option<String> {
    children(n)
        .find(|n| tag(*n) == name)
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
fn label(n: Node<'_, '_>) -> String {
    text(n, "Name")
        .unwrap_or_else(|| n.attribute("id").unwrap_or(tag(n)).into())
        .chars()
        .take(240)
        .collect()
}
fn frame(n: Node<'_, '_>) -> Option<NodeId> {
    n.ancestors()
        .find(|n| n.is_element() && tag(*n).ends_with("Frame"))
        .map(|n| n.id())
}
fn frame_ref<'a, 'b>(n: Node<'a, 'b>) -> Option<&'a str> {
    n.ancestors()
        .find(|a| a.is_element() && tag(*a).ends_with("Frame"))
        .and_then(|a| a.attribute("id"))
}
fn key(v: &impl serde::Serialize) -> String {
    hash(&serde_json::to_vec(v).expect("serializable"))
}

#[derive(Default)]
pub struct Projection {
    pub elapsed_ms: u128,
    facts: Vec<Fact>,
    anchors: HashMap<String, usize>,
    qualifiers: HashMap<NodeId, String>,
    edges: BTreeSet<(String, String, String)>,
    stops: BTreeSet<(String, String, String)>,
    journeys: Vec<Journey>,
}
struct Fact {
    anchor: String,
    noc: String,
    kind: String,
    key: String,
    name: String,
    summary: Value,
    detail: Value,
    reference: String,
    version: String,
}
struct Journey {
    ticket: String,
    route: String,
    origin: String,
    destination: String,
    amount: String,
    currency: String,
    reference: String,
}
struct Graph<'a, 'b> {
    index: HashMap<String, Vec<Node<'a, 'b>>>,
    digest: HashMap<NodeId, String>,
    known: HashSet<NodeId>,
    issues: BTreeSet<String>,
}
impl<'a, 'b> Graph<'a, 'b> {
    fn new(doc: &'a Document<'b>) -> Self {
        let mut index: HashMap<String, Vec<Node<'a, 'b>>> = HashMap::new();
        let mut known = HashSet::new();
        for n in doc.descendants().filter(|n| n.is_element()) {
            if n.tag_name().namespace() != Some(NS)
                || n.parent()
                    .is_some_and(|p| p.is_element() && !known.contains(&p.id()))
            {
                continue;
            }
            known.insert(n.id());
            if let Some(id) = n.attribute("id") {
                index.entry(id.into()).or_default().push(n);
            }
        }
        Self {
            index,
            known,
            digest: HashMap::new(),
            issues: BTreeSet::new(),
        }
    }
    fn resolve(&mut self, r: Node<'a, 'b>) -> Option<Node<'a, 'b>> {
        let reference = r.attribute("ref")?;
        if !self.known.contains(&r.id()) {
            self.issues.insert(format!(
                "unsupported_extension_reference:{}:{}",
                tag(r),
                reference
            ));
            return None;
        }
        if r.attribute("version").is_some()
            && r.attribute("versionRef").is_some()
            && r.attribute("version") != r.attribute("versionRef")
        {
            self.issues.insert(format!(
                "conflicting_reference_versions:{}:{}",
                tag(r),
                reference
            ));
            return None;
        }
        let requested_version = r.attribute("version").or_else(|| r.attribute("versionRef"));
        let kind = tag(r).strip_suffix("Ref").unwrap_or(tag(r));
        let matches_kind = |n: &Node<'a, 'b>| match kind {
            "StartTariffZone" | "EndTariffZone" | "StartFareZone" | "EndFareZone" => {
                matches!(tag(*n), "FareZone" | "TariffZone")
            }
            "FareProduct" => matches!(
                tag(*n),
                "PreassignedFareProduct" | "FareProduct" | "AmountOfPriceUnitProduct"
            ),
            _ => tag(*n) == kind,
        };
        let mut candidates: Vec<_> = self
            .index
            .get(reference)
            .into_iter()
            .flatten()
            .copied()
            .filter(matches_kind)
            .filter(|n| requested_version.is_none_or(|v| n.attribute("version") == Some(v)))
            .filter(|n| {
                r.attribute("frameRef").is_none_or(|f| {
                    n.ancestors().any(|a| {
                        a.attribute("id") == Some(f) && a.is_element() && tag(a).ends_with("Frame")
                    })
                })
            })
            .collect();
        if r.attribute("frameRef").is_none() && candidates.iter().any(|n| frame(*n) == frame(r)) {
            candidates.retain(|n| frame(*n) == frame(r));
        }
        if candidates.len() == 1 {
            return Some(candidates[0]);
        }
        // Standard vocabulary refs are retained verbatim if not locally defined.
        if !reference.starts_with("fxc:")
            && !(candidates.is_empty()
                && ((matches!(kind, "ScheduledStopPoint" | "StopPlace")
                    && crate::netex::stop_code(reference).is_some())
                    || (kind == "TopographicProjection" && reference.starts_with("nptgLocality:"))))
        {
            self.issues.insert(format!(
                "{}:{}:{}:{}",
                if candidates.is_empty() {
                    "unresolved"
                } else {
                    "ambiguous"
                },
                tag(r),
                reference,
                requested_version.unwrap_or("*")
            ));
        }
        None
    }
    fn canonical(&mut self, n: Node<'a, 'b>, stack: &mut HashSet<NodeId>) -> String {
        if let Some(v) = self.digest.get(&n.id()) {
            return v.clone();
        }
        if !stack.insert(n.id()) {
            return key(&("cycle", tag(n), n.attribute("id")));
        }
        let attrs: BTreeMap<_, _> = n
            .attributes()
            .filter(|a| {
                !(self.known.contains(&n.id())
                    && a.namespace().is_none()
                    && matches!(
                        a.name(),
                        "id" | "version"
                            | "ref"
                            | "created"
                            | "changed"
                            | "dataSourceRef"
                            | "responsibilitySetRef"
                    ))
            })
            .map(|a| {
                (
                    a.namespace()
                        .map(|ns| format!("{{{ns}}}{}", a.name()))
                        .unwrap_or_else(|| a.name().into()),
                    a.value(),
                )
            })
            .collect();
        let target = if self.known.contains(&n.id()) && n.attribute("ref").is_some() {
            if let Some(t) = self.resolve(n) {
                self.canonical(t, stack)
            } else {
                key(&(
                    n.attribute("ref"),
                    n.attribute("version"),
                    frame(n).map(|_| "document"),
                ))
            }
        } else {
            String::new()
        };
        let child: Vec<_> = n
            .children()
            .filter(|c| c.is_element())
            .map(|c| self.canonical(c, stack))
            .collect();
        let txt = if children(n).next().is_none() {
            n.text().unwrap_or("").trim()
        } else {
            ""
        };
        let digest = key(&(n.tag_name().namespace(), tag(n), attrs, target, txt, child));
        let digest = if n.children().any(|c| c.is_element())
            && n.children()
                .any(|c| c.is_text() && !c.text().unwrap_or("").trim().is_empty())
        {
            let mixed: Vec<_> = n
                .children()
                .filter_map(|c| {
                    if c.is_element() {
                        Some(json!(("element", self.canonical(c, stack))))
                    } else if c.is_text() && !c.text().unwrap_or("").trim().is_empty() {
                        Some(json!(("text", c.text().unwrap().trim())))
                    } else {
                        None
                    }
                })
                .collect();
            key(&(digest, mixed))
        } else {
            digest
        };
        stack.remove(&n.id());
        self.digest.insert(n.id(), digest.clone());
        digest
    }
    fn closure(&mut self, roots: &[Node<'a, 'b>], follow_tariffs: bool) -> Vec<Node<'a, 'b>> {
        let mut seen = HashSet::new();
        let mut todo = roots.to_vec();
        let mut out = Vec::new();
        while let Some(n) = todo.pop() {
            if !seen.insert(n.id()) {
                continue;
            }
            out.push(n);
            for c in n
                .children()
                .filter(|c| c.is_element() && c.tag_name().namespace() != Some(NS))
            {
                self.issues.insert(format!(
                    "unsupported_extension:{}:{}",
                    c.tag_name().namespace().unwrap_or(""),
                    tag(c)
                ));
            }
            for a in n.attributes().filter(|a| {
                a.namespace().is_some_and(|ns| {
                    ns != "http://www.w3.org/XML/1998/namespace"
                        && ns != "http://www.w3.org/2001/XMLSchema-instance"
                })
            }) {
                self.issues.insert(format!(
                    "unsupported_extension_attribute:{}:{}",
                    a.namespace().unwrap(),
                    a.name()
                ));
            }
            todo.extend(children(n));
            if n.attribute("ref").is_some()
                && (follow_tariffs || tag(n) != "TariffRef")
                && let Some(t) = self.resolve(n)
            {
                if !follow_tariffs
                    && matches!(
                        tag(n),
                        "PreassignedFareProductRef"
                            | "FareProductRef"
                            | "AmountOfPriceUnitProductRef"
                    )
                    && roots.first().is_some_and(|p| p.id() != t.id())
                {
                    self.issues.insert(format!(
                        "unsupported_multi_product_sales_context:{}:{}",
                        tag(n),
                        n.attribute("ref").unwrap_or("")
                    ));
                    continue;
                }
                todo.push(t);
            }
        }
        out
    }
}
fn access_signature<'a, 'b>(n: Node<'a, 'b>, graph: &mut Graph<'a, 'b>) -> String {
    if matches!(tag(n), "FareZoneRef" | "TariffZoneRef") {
        if let Some(zone) = graph.resolve(n) {
            let (members, complete) = stops(zone);
            return key(&(members.keys().collect::<Vec<_>>(), complete));
        }
    }
    let attrs: BTreeMap<_, _> = n
        .attributes()
        .filter(|a| {
            !(a.namespace().is_none()
                && matches!(
                    a.name(),
                    "id" | "version"
                        | "created"
                        | "changed"
                        | "dataSourceRef"
                        | "responsibilitySetRef"
                ))
        })
        .map(|a| (a.name(), a.value()))
        .collect();
    let child: Vec<_> = children(n)
        .filter(|c| !matches!(tag(*c), "Name" | "Description"))
        .map(|c| access_signature(c, graph))
        .collect();
    key(&(
        tag(n),
        attrs,
        if children(n).next().is_none() {
            n.text().unwrap_or("").trim()
        } else {
            ""
        },
        child,
    ))
}
fn anchor(n: Node<'_, '_>, kind: &str, noc: &str) -> String {
    // Stable across version changes when the source ref is unique; duplicates get
    // their own position (and are never silently treated as equivalent).
    format!(
        "{kind}:{noc}:{}:{}",
        tag(n),
        n.attribute("id").unwrap_or("")
    )
}
fn stops(n: Node<'_, '_>) -> (BTreeMap<String, String>, bool) {
    let mut result = BTreeMap::new();
    let mut complete = true;
    for member in children(n).filter(|c| tag(*c) == "members") {
        for r in member.children().filter(|c| c.is_element()) {
            if r.tag_name().namespace() == Some(NS)
                && matches!(tag(r), "ScheduledStopPointRef" | "StopPlaceRef")
                && let Some(code) = r.attribute("ref").and_then(crate::netex::stop_code)
            {
                result.insert(code, r.text().unwrap_or("").trim().to_string());
            } else {
                complete = false;
            }
        }
    }
    if text(n, "ScopingMethod").is_some_and(|v| v != "explicitStops") {
        complete = false;
    }
    complete &= !result.is_empty();
    (result, complete)
}
fn terms(ns: &[Node<'_, '_>]) -> BTreeMap<String, BTreeSet<String>> {
    let mut result: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for n in ns {
        if children(*n).next().is_none()
            && !matches!(
                tag(*n),
                "Name"
                    | "Description"
                    | "Amount"
                    | "Currency"
                    | "DistanceMatrixElementRef"
                    | "GeographicalIntervalPriceRef"
                    | "ScheduledStopPointRef"
                    | "StopPlaceRef"
                    | "StartTariffZoneRef"
                    | "EndTariffZoneRef"
                    | "StartFareZoneRef"
                    | "EndFareZoneRef"
            )
        {
            let value = n.attribute("ref").or_else(|| n.text()).unwrap_or("").trim();
            if !value.is_empty() {
                result
                    .entry(tag(*n).into())
                    .or_default()
                    .insert(value.chars().take(512).collect());
            }
        }
    }
    result
}
fn single_node<'a, 'b>(
    mut nodes: impl Iterator<Item = Node<'a, 'b>>,
) -> std::result::Result<Option<Node<'a, 'b>>, ()> {
    let first = nodes.next();
    if nodes.next().is_some() {
        Err(())
    } else {
        Ok(first)
    }
}
fn conflicting_selectors<'a, 'b>(graph: &mut Graph<'a, 'b>, context: Node<'a, 'b>) -> bool {
    let mut bindings: HashMap<&str, HashSet<(Option<NodeId>, String, String)>> = HashMap::new();
    for r in nodes(context).filter(|n| {
        matches!(
            tag(*n),
            "UserProfileRef"
                | "SalesOfferPackageRef"
                | "DistributionChannelRef"
                | "PreassignedFareProductRef"
                | "FareProductRef"
                | "AmountOfPriceUnitProductRef"
        )
    }) {
        let binding = graph
            .resolve(r)
            .map(|n| (Some(n.id()), String::new(), String::new()))
            .unwrap_or_else(|| {
                (
                    None,
                    r.attribute("ref").unwrap_or("").into(),
                    r.attribute("version")
                        .or_else(|| r.attribute("versionRef"))
                        .unwrap_or("")
                        .into(),
                )
            });
        bindings.entry(tag(r)).or_default().insert(binding);
    }
    bindings.values().any(|b| b.len() > 1)
}
fn currency(n: Node<'_, '_>, defaults: &HashMap<NodeId, String>) -> Option<String> {
    text(n, "Currency")
        .or_else(|| n.attribute("currency").map(str::to_string))
        .or_else(|| n.ancestors().find_map(|a| defaults.get(&a.id()).cloned()))
}

impl Projection {
    fn source_anchor(&self, n: Node<'_, '_>, kind: &str, noc: &str) -> String {
        let base = anchor(n, kind, noc);
        self.qualifiers
            .get(&n.id())
            .map(|q| format!("{base}:scope:{q}"))
            .unwrap_or(base)
    }
    fn fact(
        &mut self,
        n: Node<'_, '_>,
        noc: &str,
        kind: &str,
        semantic: Value,
        summary: Value,
        detail: Value,
    ) -> String {
        self.fact_at(
            n,
            noc,
            kind,
            semantic,
            summary,
            detail,
            self.source_anchor(n, kind, noc),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn fact_at(
        &mut self,
        n: Node<'_, '_>,
        noc: &str,
        kind: &str,
        semantic: Value,
        summary: Value,
        detail: Value,
        mut a: String,
    ) -> String {
        let count = self.anchors.entry(a.clone()).or_default();
        if *count > 0 {
            a = format!("{a}:{}", *count);
        }
        *count += 1;
        self.facts.push(Fact {
            anchor: a.clone(),
            noc: noc.into(),
            kind: kind.into(),
            key: key(&semantic),
            name: label(n),
            summary,
            detail,
            reference: n.attribute("id").unwrap_or("").into(),
            version: n.attribute("version").unwrap_or("").into(),
        });
        a
    }
}
pub fn derive(doc: &Document<'_>, defaults: &HashMap<NodeId, String>) -> Projection {
    let started = std::time::Instant::now();
    let mut graph = Graph::new(doc);
    let mut out = Projection::default();
    let all: Vec<_> = nodes(doc.root_element())
        .filter(|n| graph.known.contains(&n.id()))
        .collect();
    let mut typed_counts: HashMap<(&str, &str), usize> = HashMap::new();
    for n in &all {
        if let Some(id) = n.attribute("id") {
            *typed_counts.entry((tag(*n), id)).or_default() += 1;
        }
    }
    let mut scoped_counts = HashMap::new();
    for n in &all {
        if let Some(id) = n.attribute("id")
            && typed_counts[&(tag(*n), id)] > 1
        {
            *scoped_counts
                .entry((tag(*n), id, n.attribute("version"), frame_ref(*n)))
                .or_insert(0usize) += 1;
        }
    }
    for n in &all {
        if let Some(id) = n.attribute("id")
            && typed_counts[&(tag(*n), id)] > 1
        {
            let scope = frame_ref(*n);
            let version = n.attribute("version");
            // Identical type/ref/version/frame duplicates have no trustworthy
            // continuity when their content changes. Never assign their URLs
            // according to whichever definition happens to appear first.
            let content = if scoped_counts[&(tag(*n), id, version, scope)] > 1 {
                Some(hash(&doc.input_text().as_bytes()[n.range()]))
            } else {
                None
            };
            out.qualifiers
                .insert(n.id(), key(&(version, scope, content)));
        }
    }

    let products: Vec<_> = all
        .iter()
        .copied()
        .filter(|n| {
            matches!(
                tag(*n),
                "PreassignedFareProduct"
                    | "FareProduct"
                    | "AmountOfPriceUnitProduct"
                    | "SupplementProduct"
                    | "SalesDiscountRight"
                    | "CappedDiscountRight"
            ) && n.attribute("id").is_some()
        })
        .collect();
    let operators: BTreeSet<_> = all
        .iter()
        .filter(|n| tag(**n) == "Operator")
        .filter_map(|n| {
            n.attribute("id")
                .and_then(|s| s.strip_prefix("noc:"))
                .map(str::to_string)
                .or_else(|| {
                    if n.attribute("id").is_some_and(|s| s.starts_with("uic:")) {
                        None
                    } else {
                        text(*n, "PublicCode")
                    }
                })
        })
        .collect();
    // Reverse associations only through correctly scoped, typed product refs.
    let mut attached: HashMap<NodeId, Vec<Node<'_, '_>>> = HashMap::new();
    let mut reverse_issues: HashMap<NodeId, BTreeSet<String>> = HashMap::new();
    for r in all.iter().copied().filter(|n| {
        matches!(
            tag(*n),
            "PreassignedFareProductRef" | "FareProductRef" | "AmountOfPriceUnitProductRef"
        )
    }) {
        let resolved = graph.resolve(r);
        if resolved.is_none()
            && let Some(reference) = r.attribute("ref")
        {
            for p in products
                .iter()
                .filter(|p| p.attribute("id") == Some(reference))
            {
                reverse_issues.entry(p.id()).or_default().insert(format!(
                    "unresolved_product_binding:{}:{}",
                    reference,
                    r.attribute("version").unwrap_or("*")
                ));
            }
        }
        if let Some(product) = resolved
            && let Some(owner) = r.ancestors().find(|n| {
                matches!(tag(*n), "FareTable" | "SalesOfferPackage") && n.attribute("id").is_some()
            })
        {
            attached.entry(product.id()).or_default().push(owner);
        }
    }
    let mut variants = Vec::new();
    for product in products {
        let owners = attached.get(&product.id()).cloned().unwrap_or_default();
        let tables: Vec<_> = owners
            .iter()
            .copied()
            .filter(|n| tag(*n) == "FareTable")
            .collect();
        let contexts = if tables.is_empty() { owners } else { tables };
        if contexts.is_empty() {
            variants.push((product, None));
        } else {
            let mut seen = HashSet::new();
            for context in contexts {
                if seen.insert(context.id()) {
                    variants.push((product, Some(context)));
                }
            }
        }
    }
    let mut selector_cache = HashMap::new();
    for (product, context) in variants {
        graph.issues = reverse_issues
            .get(&product.id())
            .cloned()
            .unwrap_or_default();
        for owner in std::iter::once(product).chain(context) {
            if let Some(id) = owner.attribute("id")
                && scoped_counts
                    .get(&(tag(owner), id, owner.attribute("version"), frame_ref(owner)))
                    .is_some_and(|n| *n > 1)
            {
                graph.issues.insert(format!(
                    "ambiguous_source_definition:{}:{}:{}",
                    tag(owner),
                    id,
                    owner.attribute("version").unwrap_or("")
                ));
            }
        }

        let mut roots = vec![product];
        roots.extend(context);
        let mut connected = graph.closure(&roots, true);
        // Tariff validity and line restrictions apply to its referenced structures.
        let tariffs: HashSet<_> = connected
            .iter()
            .flat_map(|n| {
                n.ancestors()
                    .filter(|a| tag(*a) == "Tariff")
                    .map(|a| a.id())
            })
            .collect();
        for t in tariffs.iter().filter_map(|id| doc.get_node(*id)) {
            roots.extend(children(t).filter(|c| {
                !matches!(
                    tag(*c),
                    "fareStructureElements"
                        | "geographicalIntervals"
                        | "timeIntervals"
                        | "Name"
                        | "Description"
                )
            }));
        }
        // Effective validity can belong to a referenced network/product/price
        // frame, not only the product's own parent. Collect each owned frame
        // once; shared FXC vocabulary publication dates are metadata.
        let mut frames = HashSet::new();
        for n in connected.iter().filter(|n| n.attribute("id").is_some()) {
            for a in n.ancestors().filter(|a| {
                a.is_element()
                    && tag(*a).ends_with("Frame")
                    && !a.attribute("id").is_some_and(|id| id.starts_with("fxc:"))
            }) {
                frames.insert(a.id());
            }
        }
        for a in frames.iter().filter_map(|id| doc.get_node(*id)) {
            roots.extend(
                children(a).filter(|c| matches!(tag(*c), "ValidBetween" | "validityConditions")),
            );
        }
        connected = graph.closure(&roots, true);
        let applicable = graph.closure(&roots, false);
        let mut nocs: BTreeSet<String> = applicable
            .iter()
            .filter(|n| tag(**n) == "OperatorRef")
            .filter_map(|n| {
                n.attribute("ref")
                    .and_then(|s| s.strip_prefix("noc:"))
                    .map(str::to_string)
            })
            .collect();
        for operator in applicable.iter().filter(|n| {
            tag(**n) == "Operator" && !n.attribute("id").is_some_and(|r| r.starts_with("uic:"))
        }) {
            if let Some(code) = text(*operator, "PublicCode").or_else(|| {
                operator
                    .attribute("id")
                    .and_then(|v| v.strip_prefix("noc:"))
                    .map(str::to_string)
            }) {
                nocs.insert(code);
            }
        }
        if nocs.is_empty() && operators.len() == 1 {
            nocs = operators.clone();
        }
        if nocs.is_empty() {
            continue;
        }
        let declared_type = text(product, "ProductType");
        let standard_type = nodes(product)
            .find(|n| tag(*n) == "TypeOfFareProductRef")
            .and_then(|n| match n.attribute("ref") {
                Some("fxc:standard_product@trip@single") => Some("singleTrip"),
                Some("fxc:standard_product@trip@day_return") => Some("dayReturnTrip"),
                Some("fxc:standard_product@trip@period_return") => Some("periodReturnTrip"),
                Some("fxc:standard_product@pass@period") => Some("periodPass"),
                _ => None,
            });
        let type_source = if declared_type.is_some() {
            "ProductType"
        } else if standard_type.is_some() {
            "TypeOfFareProductRef"
        } else {
            "unclassified"
        };
        let ptype = declared_type.unwrap_or_else(|| standard_type.unwrap_or("unclassified").into());
        let travel = matches!(
            ptype.as_str(),
            "singleTrip"
                | "dayReturnTrip"
                | "periodReturnTrip"
                | "returnTrip"
                | "periodPass"
                | "multiTrip"
                | "carnet"
        );
        let condition_terms = terms(&applicable);
        let mut digests: Vec<_> = roots
            .iter()
            .map(|n| graph.canonical(*n, &mut HashSet::new()))
            .collect();
        digests.sort();
        digests.dedup();
        // Unresolved references are scoped to the publication to avoid false merges.
        let selector_conflict = context.is_some_and(|c| {
            *selector_cache
                .entry(c.id())
                .or_insert_with(|| conflicting_selectors(&mut graph, c))
        });
        let mut issues: Vec<_> = graph.issues.iter().cloned().collect();
        if selector_conflict {
            issues.push("unsupported_nested_sales_selectors".into());
        }

        let prices: Vec<_> = applicable
            .iter()
            .copied()
            .filter(|n| tag(*n).ends_with("Price"))
            .collect();
        let mut fixed = BTreeSet::new();
        for p in &prices {
            if tag(*p) != "DistanceMatrixElementPrice" && text(*p, "Amount").is_some() {
                fixed.insert((text(*p, "Amount").unwrap(), currency(*p, defaults)));
            }
        }
        let price_values: Vec<_> = fixed
            .iter()
            .map(|(a, c)| json!({"amount":a,"currency":c}))
            .collect();
        let durations:Vec<_>=applicable.iter().filter(|n|tag(**n)=="TimeInterval" || tag(**n)=="UsageValidityPeriod").map(|n|json!({"name":text(*n,"Name"),"duration":text(*n,"StandardDuration").or_else(||text(*n,"Description")),"start_time":text(*n,"StartTime"),"end_time":text(*n,"EndTime")})).collect();
        let conditions_truncated =
            condition_terms.len() > 128 || condition_terms.values().any(|v| v.len() > 16);
        let conditions_json: BTreeMap<_, _> = condition_terms
            .iter()
            .take(128)
            .map(|(k, v)| (k, v.iter().take(16).collect::<Vec<_>>()))
            .collect();
        let compact_terms = |k: &str| {
            condition_terms
                .get(k)
                .map(|v| v.iter().take(8).collect::<Vec<_>>())
        };
        // Keep HTTP summaries bounded; full evidence remains accessible via sources.
        let summary = json!({"product_type":ptype,"product_type_source":type_source,"travel":travel,"price_basis":if selector_conflict {"conditional"}else if applicable.iter().any(|n|tag(*n)=="DistanceMatrixElementPrice"){"journey"}else if fixed.len()==1{"published"}else{"conditional"},"prices":price_values.iter().take(8).collect::<Vec<_>>(),"prices_truncated":price_values.len()>8,"eligibility":compact_terms("UserType"),"sales_channels":compact_terms("DistributionChannelType"),"duration":compact_terms("StandardDuration").or_else(||compact_terms("Duration")),"duration_rules":durations.iter().take(8).collect::<Vec<_>>(),"line_refs":compact_terms("LineRef"),"direction_refs":compact_terms("DirectionRef"),"valid_from":compact_terms("FromDate"),"valid_until":compact_terms("ToDate"),"conditions_evaluated":false,"resolution":if issues.is_empty(){"published_conditions"}else{"unresolved"}});
        let detail = json!({"conditions":conditions_json,"additional_tariff_evidence":connected.len()>applicable.len(),"conditions_truncated":conditions_truncated,"condition_logic":"Uninterpreted; grouping and all source restrictions remain available through provenance","unresolved":issues.iter().take(100).collect::<Vec<_>>(),"unresolved_count":issues.len(),"conditions_evaluated":false,"provenance":"Load the paginated sources endpoint for original conditions and reference paths"});
        // A resolved point/area is created once per source zone, reused by products.
        for noc in nocs {
            let family_key = json!((
                label(product)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase(),
                &ptype
            ));
            let family_anchor = format!(
                "family:{noc}:{}:{}",
                key(&family_key),
                key(&out.source_anchor(product, "family_source", &noc))
            );
            if !out.anchors.contains_key(&family_anchor) {
                out.anchors.insert(family_anchor.clone(), 1);
                out.facts.push(Fact {
                    anchor: family_anchor.clone(),
                    noc: noc.clone(),
                    kind: "family".into(),
                    key: key(&family_key),
                    name: label(product),
                    summary: json!({"product_type":ptype,"travel":travel,"equivalence":false}),
                    detail: json!({"equivalence":false}),
                    reference: product.attribute("id").unwrap().into(),
                    version: product.attribute("version").unwrap_or("").into(),
                });
            }
            let ticket_index = out.facts.len();
            let journey_start = out.journeys.len();
            let ticket = out.fact_at(
                product,
                &noc,
                "ticket",
                json!((&digests, &summary, &issues)),
                summary.clone(),
                detail.clone(),
                format!(
                    "{}:context:{}",
                    out.source_anchor(product, "ticket", &noc),
                    context
                        .map(|c| out.source_anchor(c, "context", &noc))
                        .unwrap_or_else(|| "product".into())
                ),
            );
            out.edges
                .insert((ticket.clone(), family_anchor, "family".into()));
            let mut zone_anchors = HashMap::new();
            // Only positive explicit access rules create ticket browsing areas. Matrix
            // endpoints stay points; exclusions and unknown access operators stay unknown.
            for rule in applicable.iter().copied().filter(|n| {
                travel
                    && matches!(
                        tag(*n),
                        "FareStructureElement" | "GenericParameterAssignment"
                    )
                    && !(tag(*n) == "GenericParameterAssignment"
                        && n.ancestors()
                            .skip(1)
                            .any(|a| tag(a) == "FareStructureElement"))
            }) {
                let positive = nodes(rule).any(|n| {
                    tag(n) == "TypeOfAccessRightAssignmentRef"
                        && n.attribute("ref") == Some("fxc:can_access")
                });
                let negative = nodes(rule).any(|n| {
                    tag(n) == "TypeOfAccessRightAssignmentRef"
                        && n.attribute("ref").is_some_and(|v| v != "fxc:can_access")
                });
                if !positive || negative {
                    continue;
                }
                for r in nodes(rule).filter(|n| matches!(tag(*n), "FareZoneRef" | "TariffZoneRef"))
                {
                    if let Some(zone) = graph.resolve(r) {
                        let (members, complete) = stops(zone);
                        let a = format!(
                            "area_context:{}",
                            key(&(
                                out.source_anchor(zone, "area", &noc),
                                out.source_anchor(rule, "access", &noc),
                                &ticket
                            ))
                        );
                        if !out.anchors.contains_key(&a) {
                            let restriction_nodes: Vec<_> = nodes(rule)
                                .filter(|n| {
                                    !matches!(
                                        tag(*n),
                                        "Name" | "Description" | "FareZoneRef" | "TariffZoneRef"
                                    )
                                })
                                .collect();
                            let mut restrictions = terms(&restriction_nodes);
                            for k in ["LineRef", "DirectionRef", "DirectionType"] {
                                if let Some(v) = condition_terms.get(k) {
                                    restrictions.insert(k.into(), v.clone());
                                }
                            }
                            let semantic = if complete {
                                json!((
                                    &members.keys().collect::<Vec<_>>(),
                                    &restrictions,
                                    access_signature(rule, &mut graph)
                                ))
                            } else {
                                json!((
                                    "incomplete",
                                    zone.attribute("id"),
                                    graph.canonical(zone, &mut HashSet::new())
                                ))
                            };
                            out.fact_at(zone,&noc,"area",semantic,json!({"membership":if complete{"published_stops"}else{"incomplete"},"stop_count":members.len(),"boundary_available":false}),json!({"restrictions":restrictions,"membership_is_boundary":false,"unresolved_count":issues.len(),"source_context":{"kind":tag(rule),"ref":rule.attribute("id"),"version":rule.attribute("version"),"product_ref":product.attribute("id")}}),a.clone());
                            for (code, name) in members {
                                out.stops.insert((a.clone(), code, name));
                            }
                        }
                        out.edges.insert((ticket.clone(), a, "area".into()));
                    }
                }
            }
            let lines: Vec<_> = applicable
                .iter()
                .copied()
                .filter(|n| tag(*n) == "Line")
                .collect();
            let typed_direction: BTreeSet<_> = applicable
                .iter()
                .filter(|n| matches!(tag(**n), "DirectionType" | "DirectionRef" | "DirectionName"))
                .map(|n| {
                    (
                        tag(*n),
                        n.attribute("ref").or_else(|| n.text()).unwrap_or(""),
                    )
                })
                .collect();
            let direction: BTreeSet<_> = typed_direction.iter().map(|(_, value)| *value).collect();
            let mut routes = Vec::new();
            let mut lookup_issues = BTreeSet::new();
            for line in lines {
                let reference = line.attribute("id").unwrap_or("");
                let a = format!(
                    "route_context:{}",
                    key(&(out.source_anchor(line, "route", &noc), &typed_direction))
                );
                if !out.anchors.contains_key(&a) {
                    out.fact_at(line,&noc,"route",json!((&reference,&typed_direction)),json!({"line":text(line,"PublicCode"),"directions":direction}),json!({"line_ref":reference,"directions":direction,"direction_conditions":typed_direction}),a.clone());
                }
                routes.push(a.clone());
                out.edges.insert((ticket.clone(), a, "route".into()));
            }
            for p in &prices {
                if routes.len() != 1
                    || selector_conflict
                    || issues
                        .iter()
                        .any(|i| i.starts_with("unsupported_multi_product_sales_context"))
                {
                    continue;
                }
                if tag(*p) != "DistanceMatrixElementPrice" {
                    continue;
                }
                let mref = match single_node(
                    nodes(*p).filter(|n| tag(*n) == "DistanceMatrixElementRef"),
                ) {
                    Ok(Some(r)) => r,
                    Ok(None) => continue,
                    Err(()) => {
                        lookup_issues.insert("ambiguous_matrix_reference");
                        continue;
                    }
                };
                let Some(matrix) = graph.resolve(mref) else {
                    continue;
                };
                let endpoints = (
                    single_node(
                        nodes(matrix).filter(|n| {
                            matches!(tag(*n), "StartTariffZoneRef" | "StartFareZoneRef")
                        }),
                    ),
                    single_node(
                        nodes(matrix)
                            .filter(|n| matches!(tag(*n), "EndTariffZoneRef" | "EndFareZoneRef")),
                    ),
                );
                let (start_ref, end_ref) = match endpoints {
                    (Ok(Some(a)), Ok(Some(b))) => (a, b),
                    (Err(()), _) | (_, Err(())) => {
                        lookup_issues.insert("ambiguous_matrix_endpoints");
                        continue;
                    }
                    _ => continue,
                };
                let (Some(start), Some(end)) = (graph.resolve(start_ref), graph.resolve(end_ref))
                else {
                    continue;
                };
                let price_ref =
                    match single_node(nodes(*p).filter(|n| tag(*n).ends_with("PriceRef"))) {
                        Ok(r) => r,
                        Err(()) => {
                            lookup_issues.insert("ambiguous_price_reference");
                            continue;
                        }
                    };
                let monetary = if text(*p, "Amount").is_some() {
                    if price_ref.is_some() {
                        lookup_issues.insert("ambiguous_price_binding");
                        continue;
                    }
                    Some(*p)
                } else {
                    price_ref.and_then(|r| graph.resolve(r))
                };
                let Some(monetary) = monetary else {
                    continue;
                };
                let (Some(amount), Some(currency)) =
                    (text(monetary, "Amount"), currency(monetary, defaults))
                else {
                    continue;
                };
                for zone in [start, end] {
                    if zone_anchors.contains_key(&zone.id()) {
                        continue;
                    }
                    let a = out.source_anchor(zone, "point", &noc);
                    let (members, complete) = stops(zone);
                    if !out.anchors.contains_key(&a) {
                        out.fact(zone,&noc,"point",json!((zone.attribute("id"),&members,complete)),json!({"stop_count":members.len(),"membership":if complete{"published_stops"}else{"incomplete"}}),json!({"source_zone":zone.attribute("id")}));
                        for (code, name) in members {
                            out.stops.insert((a.clone(), code, name));
                        }
                    }
                    zone_anchors.insert(zone.id(), a);
                }
                for route in &routes {
                    out.journeys.push(Journey {
                        ticket: ticket.clone(),
                        route: route.clone(),
                        origin: zone_anchors[&start.id()].clone(),
                        destination: zone_anchors[&end.id()].clone(),
                        amount: amount.clone(),
                        currency: currency.clone(),
                        reference: matrix.attribute("id").unwrap_or("").into(),
                    });
                }
            }
            let cells = prices
                .iter()
                .filter(|p| tag(**p) == "DistanceMatrixElementPrice")
                .count();
            let supported = out.journeys[journey_start..]
                .iter()
                .map(|j| &j.reference)
                .collect::<BTreeSet<_>>()
                .len();
            if cells > 0
                && (selector_conflict
                    || issues
                        .iter()
                        .any(|i| i.starts_with("unsupported_multi_product_sales_context")))
            {
                lookup_issues.insert("unsupported_matrix_sales_context");
            }
            if cells > 0 && routes.len() != 1 {
                lookup_issues.insert("unresolved_or_multiple_matrix_lines");
            }
            if !lookup_issues.is_empty() {
                out.facts[ticket_index].summary["resolution"] = json!("unresolved");
                out.facts[ticket_index].detail["lookup_issues"] = json!(lookup_issues);
                out.facts[ticket_index].detail["unresolved_count"] =
                    json!(issues.len() + lookup_issues.len());
            }
            let support = json!({"pattern":"explicit_distance_matrix_price","cells":cells,"resolved_cells":supported,"status":if cells==0 {"not_supported_or_not_journey_dependent"}else if supported==cells {"supported"}else{"partial_or_unresolved"}});
            out.facts[ticket_index].summary["journey_support"] = support;
            out.facts[ticket_index].detail["source_context"]=context.map(|c|json!({"kind":tag(c),"ref":c.attribute("id"),"version":c.attribute("version"),"frame_ref":c.ancestors().find(|a|a.is_element() && tag(*a).ends_with("Frame")).and_then(|a|a.attribute("id"))})).unwrap_or(Value::Null);
        }
    }
    out.elapsed_ms = started.elapsed().as_millis();
    out
}

/// Temporary work lists live inside the writer's transaction. They are empty on
/// unchanged imports and never touch the persistent database's zero-write path.
pub fn begin(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS temp.catalogue_previous; CREATE TEMP TABLE catalogue_previous(document_id INTEGER,anchor TEXT,object_id TEXT,noc TEXT,kind TEXT,semantic_key TEXT);")?;
    Ok(())
}
pub fn forget(conn: &Connection, document: i64) -> Result<()> {
    conn.prepare_cached("INSERT INTO temp.catalogue_previous SELECT document_id,anchor,object_id,noc,kind,semantic_key FROM catalogue_sources WHERE document_id=?")?.execute([document])?;
    for table in [
        "catalogue_sources",
        "catalogue_source_edges",
        "catalogue_source_stops",
        "catalogue_source_journeys",
    ] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE document_id=?"),
            [document],
        )?;
    }
    Ok(())
}
/// Reconcile changed publications by source identity. A price edit does not
/// rewrite unchanged zone memberships, graph edges or directional source cells.
pub fn store(conn: &Connection, document: i64, path: &str, projection: Projection) -> Result<()> {
    let mut old:BTreeMap<String,Vec<String>>=conn.prepare_cached("SELECT anchor,noc,kind,semantic_key,name,summary,detail,source_ref,source_version FROM catalogue_sources WHERE document_id=?")?.query_map([document],|r|Ok((r.get(0)?,(1..9).map(|i|r.get(i)).collect::<rusqlite::Result<_>>()?)))?.collect::<rusqlite::Result<_>>()?;
    for f in projection.facts {
        let unresolved = f
            .detail
            .get("unresolved_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
            || f.summary.get("membership").and_then(Value::as_str) == Some("incomplete");
        let semantic = if unresolved {
            key(&(&f.key, path, &f.anchor))
        } else {
            f.key
        };
        let values = vec![
            f.noc,
            f.kind,
            semantic,
            f.name,
            f.summary.to_string(),
            f.detail.to_string(),
            f.reference,
            f.version,
        ];
        if let Some(previous) = old.remove(&f.anchor) {
            if previous == values {
                continue;
            }
            conn.prepare_cached("INSERT INTO temp.catalogue_previous SELECT document_id,anchor,object_id,noc,kind,semantic_key FROM catalogue_sources WHERE document_id=? AND anchor=?")?.execute(params![document,f.anchor])?;
            conn.prepare_cached("UPDATE catalogue_sources SET noc=?3,kind=?4,semantic_key=?5,name=?6,summary=?7,detail=?8,source_ref=?9,source_version=?10,object_id=NULL WHERE document_id=?1 AND anchor=?2")?.execute(params![document,f.anchor,values[0],values[1],values[2],values[3],values[4],values[5],values[6],values[7]])?;
        } else {
            conn.prepare_cached("INSERT INTO catalogue_sources(document_id,anchor,noc,kind,semantic_key,name,summary,detail,source_ref,source_version) VALUES(?,?,?,?,?,?,?,?,?,?)")?.execute(params![document,f.anchor,values[0],values[1],values[2],values[3],values[4],values[5],values[6],values[7]])?;
        }
    }
    for a in old.keys() {
        conn.prepare_cached("INSERT INTO temp.catalogue_previous SELECT document_id,anchor,object_id,noc,kind,semantic_key FROM catalogue_sources WHERE document_id=? AND anchor=?")?.execute(params![document,a])?;
        conn.prepare_cached("DELETE FROM catalogue_sources WHERE document_id=? AND anchor=?")?
            .execute(params![document, a])?;
    }
    sync_rows(
        conn,
        document,
        "catalogue_source_edges",
        &["source_anchor", "target_anchor", "relation"],
        projection
            .edges
            .into_iter()
            .map(|(a, b, c)| vec![a, b, c])
            .collect(),
    )?;
    sync_rows(
        conn,
        document,
        "catalogue_source_stops",
        &["anchor", "code", "name"],
        projection
            .stops
            .into_iter()
            .map(|(a, b, c)| vec![a, b, c])
            .collect(),
    )?;
    sync_rows(
        conn,
        document,
        "catalogue_source_journeys",
        &[
            "anchor",
            "route_anchor",
            "origin_anchor",
            "destination_anchor",
            "amount",
            "currency",
            "matrix_ref",
        ],
        projection
            .journeys
            .into_iter()
            .map(|j| {
                vec![
                    j.ticket,
                    j.route,
                    j.origin,
                    j.destination,
                    j.amount,
                    j.currency,
                    j.reference,
                ]
            })
            .collect(),
    )?;
    Ok(())
}
fn sync_rows(
    conn: &Connection,
    document: i64,
    table: &str,
    columns: &[&str],
    new: BTreeSet<Vec<String>>,
) -> Result<()> {
    let select = format!(
        "SELECT {} FROM {table} WHERE document_id=?",
        columns.join(",")
    );
    let old: BTreeSet<Vec<String>> = conn
        .prepare_cached(&select)?
        .query_map([document], |r| {
            (0..columns.len()).map(|i| r.get(i)).collect()
        })?
        .collect::<rusqlite::Result<_>>()?;
    let insert = format!(
        "INSERT INTO {table}(document_id,{}) VALUES(?1,{})",
        columns.join(","),
        (2..columns.len() + 2)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let delete = format!(
        "DELETE FROM {table} WHERE document_id=?1 AND {}",
        columns
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{c} IS ?{}", i + 2))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    for (rows, sql) in [
        (new.difference(&old).collect::<Vec<_>>(), insert),
        (old.difference(&new).collect::<Vec<_>>(), delete),
    ] {
        let mut statement = conn.prepare_cached(&sql)?;
        for row in rows {
            let values = std::iter::once(rusqlite::types::Value::Integer(document))
                .chain(row.iter().cloned().map(rusqlite::types::Value::Text));
            statement.execute(rusqlite::params_from_iter(values))?;
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Member {
    document: i64,
    anchor: String,
    object: Option<String>,
    name: String,
    summary: String,
    detail: String,
}
fn strings(conn: &Connection, sql: &str, value: &str) -> Result<Vec<String>> {
    Ok(conn
        .prepare_cached(sql)?
        .query_map([value], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
// Keep the large directional-cell index before the three endpoint/source
// lookups. ANALYZE can otherwise choose a Cartesian product of a document's
// source facts before probing its matrices (severe on atypically large files).
const JOURNEY_SELECT: &str = "SELECT r.object_id,o.object_id,d.object_id,t.object_id,j.amount,j.currency FROM catalogue_sources t CROSS JOIN catalogue_source_journeys j ON j.document_id=t.document_id AND j.anchor=t.anchor CROSS JOIN catalogue_sources r ON r.document_id=j.document_id AND r.anchor=j.route_anchor CROSS JOIN catalogue_sources o ON o.document_id=j.document_id AND o.anchor=j.origin_anchor CROSS JOIN catalogue_sources d ON d.document_id=j.document_id AND d.anchor=j.destination_anchor WHERE t.object_id=?";

/// SQL set differences keep unchanged materialized rows untouched. `select`
/// reads only the equivalence class identified by its single bound parameter.
fn sync_materialized(
    conn: &Connection,
    table: &str,
    columns: &str,
    scope: &str,
    id: &str,
    select: &str,
) -> Result<()> {
    let delete = materialized_delete(table, columns, scope, select);
    conn.prepare_cached(&delete)?.execute(params![id, id, id])?;
    conn.prepare_cached(&format!(
        "INSERT OR IGNORE INTO {table}({columns}) {select}"
    ))?
    .execute([id])?;
    Ok(())
}
fn materialized_delete(table: &str, columns: &str, scope: &str, select: &str) -> String {
    format!(
        "DELETE FROM {table} WHERE {scope}=? AND ({columns}) IN (SELECT {columns} FROM {table} WHERE {scope}=? EXCEPT {select})"
    )
}
#[cfg(test)]
mod plan_tests {
    #[test]
    fn reconciliation_delete_is_scoped_by_ticket_index() {
        let temp = tempfile::TempDir::new().unwrap();
        let conn = crate::db::connect(&temp.path().join("plan.sqlite")).unwrap();
        let columns = "route_id,origin_id,destination_id,ticket_id,amount,currency";
        let select = format!("SELECT {columns} FROM catalogue_journeys WHERE ticket_id=?");
        let sql = super::materialized_delete("catalogue_journeys", columns, "ticket_id", &select);
        let plans: Vec<String> = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map(["ticket", "ticket", "ticket"], |r| r.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            !plans
                .iter()
                .any(|p| p.starts_with("SCAN catalogue_journeys")),
            "{plans:?}"
        );
        assert!(
            plans
                .iter()
                .filter(|p| p.contains("catalogue_journeys_ticket"))
                .count()
                >= 2,
            "{plans:?}"
        );
    }
    #[test]
    fn journey_projection_joins_cells_before_endpoint_facts() {
        let temp = tempfile::TempDir::new().unwrap();
        let conn = crate::db::connect(&temp.path().join("plan.sqlite")).unwrap();
        let plans: Vec<String> = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {}", super::JOURNEY_SELECT))
            .unwrap()
            .query_map(["ticket"], |r| r.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for alias in ["r", "o", "d"] {
            assert!(
                plans.iter().any(|p| p.contains(&format!(
                    "SEARCH {alias} USING PRIMARY KEY (document_id=? AND anchor=?)"
                ))),
                "{plans:?}"
            );
        }
    }
}
pub fn publish(conn: &Connection, generation: i64) -> Result<()> {
    let mut groups:BTreeSet<(String,String,String)>=conn.prepare("SELECT DISTINCT noc,kind,semantic_key FROM catalogue_sources WHERE object_id IS NULL UNION SELECT noc,kind,semantic_key FROM temp.catalogue_previous")?.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut affected: BTreeSet<String> = conn
        .prepare(
            "SELECT DISTINCT object_id FROM temp.catalogue_previous WHERE object_id IS NOT NULL",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    // Include unchanged contributors to a merging/splitting equivalence class.
    let mut pending: Vec<_> = affected.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        for row in conn
            .prepare_cached(
                "SELECT DISTINCT noc,kind,semantic_key FROM catalogue_sources WHERE object_id=?",
            )?
            .query_map([&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        {
            groups.insert(row?);
        }
    }
    let previous:HashMap<(i64,String),String>=conn.prepare("SELECT document_id,anchor,object_id FROM temp.catalogue_previous WHERE object_id IS NOT NULL")?.query_map([],|r|Ok(((r.get(0)?,r.get(1)?),r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    // Reserve surviving semantic keys first. This gives an unchanged partition its
    // old URL when another contributor splits away, regardless of import order.
    let mut assigned = HashMap::new();
    let mut used = HashSet::new();
    for (noc, kind, semantic) in &groups {
        let id:Option<String>=conn.query_row("SELECT id FROM catalogue_objects WHERE noc=? AND kind=? AND semantic_key=? AND active=1 ORDER BY id LIMIT 1",params![noc,kind,semantic],|r|r.get(0)).optional()?;
        let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM catalogue_sources WHERE noc=? AND kind=? AND semantic_key=?)",params![noc,kind,semantic],|r|r.get(0))?;
        if exists && let Some(id) = id {
            used.insert(id.clone());
            assigned.insert((noc.clone(), kind.clone(), semantic.clone()), id);
        }
    }
    let mut remapped_documents = BTreeSet::new();
    for (noc, kind, semantic) in groups {
        let members:Vec<Member>=conn.prepare_cached("SELECT document_id,anchor,object_id,name,summary,detail FROM catalogue_sources WHERE noc=? AND kind=? AND semantic_key=? ORDER BY name,document_id,anchor")?.query_map(params![noc,kind,semantic],|r|Ok(Member{document:r.get(0)?,anchor:r.get(1)?,object:r.get(2)?,name:r.get(3)?,summary:r.get(4)?,detail:r.get(5)?}))?.collect::<rusqlite::Result<_>>()?;
        if members.is_empty() {
            continue;
        }
        let group = (noc.clone(), kind.clone(), semantic.clone());
        let candidates: BTreeSet<String> = members
            .iter()
            .filter_map(|m| {
                m.object
                    .clone()
                    .or_else(|| previous.get(&(m.document, m.anchor.clone())).cloned())
            })
            .collect();
        let id = if let Some(id) = assigned.get(&group) {
            id.clone()
        } else if let Some(id) = candidates.iter().find(|id| !used.contains(*id)) {
            id.clone()
        } else if let Some(id)=conn.query_row("SELECT id FROM catalogue_objects WHERE noc=? AND kind=? AND semantic_key=? AND active=0 ORDER BY id LIMIT 1",params![noc,kind,semantic],|r|r.get::<_,String>(0)).optional()? {
            id
        } else {
            let mut id = format!("{kind}_{}", key(&(&noc, &semantic)));
            let mut suffix = 0;
            while used.contains(&id)
                || conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM catalogue_objects WHERE id=? AND semantic_key<>?)",
                    params![id, semantic],
                    |r| r.get::<_, bool>(0),
                )?
            {
                suffix += 1;
                id = format!("{kind}_{}", key(&(&noc, &semantic, suffix)));
            }
            id
        };
        used.insert(id.clone());
        affected.insert(id.clone());
        affected.extend(candidates);
        let representative = &members[0];
        conn.prepare_cached("INSERT OR IGNORE INTO catalogue_events SELECT ?,id,id,'restored',noc FROM catalogue_objects WHERE id=? AND active=0")?.execute(params![generation,id])?;
        conn.prepare_cached("INSERT INTO catalogue_objects(id,kind,noc,semantic_key,name,search,summary,detail,source_count,travel) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET semantic_key=excluded.semantic_key,name=excluded.name,search=excluded.search,summary=excluded.summary,detail=excluded.detail,source_count=excluded.source_count,active=1,travel=json_extract(excluded.summary,'$.travel') IS 1,revision=revision+1 WHERE semantic_key<>excluded.semantic_key OR name<>excluded.name OR summary<>excluded.summary OR detail<>excluded.detail OR source_count<>excluded.source_count OR active<>1")?.execute(params![id,kind,noc,semantic,representative.name,representative.name.to_lowercase(),representative.summary,representative.detail,members.len() as i64,serde_json::from_str::<Value>(&representative.summary)?["travel"].as_bool().unwrap_or(false)])?;
        for m in members {
            let previous_id = m
                .object
                .as_ref()
                .or_else(|| previous.get(&(m.document, m.anchor.clone())));
            if previous_id != Some(&id) {
                remapped_documents.insert(m.document);
            }
            if let Some(old) = m
                .object
                .as_ref()
                .or_else(|| previous.get(&(m.document, m.anchor.clone())))
                && old != &id
            {
                conn.prepare_cached(
                    "INSERT OR IGNORE INTO catalogue_events(generation,previous_id,current_id,reason,noc) VALUES(?,?,?,'repartition',?)",
                )?
                .execute(params![generation, old, id,noc])?;
            }
            if m.object.as_ref() != Some(&id) {
                conn.prepare_cached(
                    "UPDATE catalogue_sources SET object_id=? WHERE document_id=? AND anchor=?",
                )?
                .execute(params![id, m.document, m.anchor])?;
            }
        }
    }
    // Edges/stop sets are indexed materializations, not page-time reconstructions.
    // Include tickets in changed endpoint/route documents so their journey rows
    // follow an explicit merge/split even if the ticket signature is unchanged.
    let mut refresh = affected.clone();
    for document in remapped_documents {
        let tickets:Vec<String>=conn.prepare_cached("SELECT DISTINCT object_id FROM catalogue_sources WHERE document_id=? AND kind='ticket' AND object_id IS NOT NULL")?.query_map([document],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?;
        refresh.extend(tickets);
    }
    let mut navigation = BTreeSet::new();
    for id in &refresh {
        navigation.extend(strings(
            conn,
            "SELECT target_id FROM catalogue_edges WHERE source_id=?",
            id,
        )?);
        sync_materialized(
            conn,
            "catalogue_edges",
            "source_id,target_id,relation,search,travel",
            "source_id",
            id,
            "SELECT s.object_id,t.object_id,e.relation,o.search,o.travel FROM catalogue_sources s JOIN catalogue_objects o ON o.id=s.object_id JOIN catalogue_source_edges e ON e.document_id=s.document_id AND e.source_anchor=s.anchor JOIN catalogue_sources t ON t.document_id=e.document_id AND t.anchor=e.target_anchor WHERE s.object_id=?",
        )?;
        conn.prepare_cached("UPDATE catalogue_objects SET family_id=(SELECT CASE WHEN count(*)=1 THEN min(target_id) END FROM catalogue_edges WHERE source_id=? AND relation='family') WHERE id=? AND family_id IS NOT (SELECT CASE WHEN count(*)=1 THEN min(target_id) END FROM catalogue_edges WHERE source_id=? AND relation='family')")?.execute(params![id,id,id])?;
        sync_materialized(
            conn,
            "catalogue_stops",
            "object_id,code,name",
            "object_id",
            id,
            "SELECT s.object_id,p.code,max(p.name) FROM catalogue_sources s JOIN catalogue_source_stops p ON p.document_id=s.document_id AND p.anchor=s.anchor WHERE s.object_id=? GROUP BY p.code",
        )?;
        sync_materialized(
            conn,
            "catalogue_journeys",
            "route_id,origin_id,destination_id,ticket_id,amount,currency",
            "ticket_id",
            id,
            JOURNEY_SELECT,
        )?;
    }
    for id in &refresh {
        navigation.extend(strings(
            conn,
            "SELECT target_id FROM catalogue_edges WHERE source_id=?",
            id,
        )?);
    }
    affected.extend(navigation);
    for id in &affected {
        if !id.starts_with("area_") {
            continue;
        }
        sync_materialized(
            conn,
            "catalogue_area_families",
            "area_id,family_id,variant_count,search,travel",
            "area_id",
            id,
            "SELECT a.target_id,f.target_id,count(DISTINCT a.source_id),o.search,o.travel FROM catalogue_edges a JOIN catalogue_edges f ON f.source_id=a.source_id AND f.relation='family' JOIN catalogue_objects o ON o.id=f.target_id WHERE a.target_id=? AND a.relation='area' GROUP BY f.target_id",
        )?;
        sync_materialized(
            conn,
            "catalogue_area_ticket_variants",
            "area_id,family_id,ticket_id,search,travel",
            "area_id",
            id,
            "SELECT a.target_id,f.target_id,a.source_id,o.search,o.travel FROM catalogue_edges a JOIN catalogue_edges f ON f.source_id=a.source_id AND f.relation='family' JOIN catalogue_objects o ON o.id=a.source_id WHERE a.target_id=? AND a.relation='area'",
        )?;
    }
    let mut routes = BTreeSet::new();
    for id in &refresh {
        for route in strings(
            conn,
            "SELECT DISTINCT route_id FROM catalogue_journeys WHERE ticket_id=?",
            id,
        )? {
            routes.insert(route);
        }
        if id.starts_with("route_") {
            routes.insert(id.clone());
        }
    }
    for route in routes {
        sync_materialized(
            conn,
            "catalogue_route_origins",
            "route_id,origin_id",
            "route_id",
            &route,
            "SELECT route_id,origin_id FROM catalogue_journeys WHERE route_id=? GROUP BY origin_id",
        )?;
        sync_materialized(
            conn,
            "catalogue_route_pairs",
            "route_id,origin_id,destination_id",
            "route_id",
            &route,
            "SELECT route_id,origin_id,destination_id FROM catalogue_journeys WHERE route_id=? GROUP BY origin_id,destination_id",
        )?;
    }
    for id in &affected {
        conn.prepare_cached("UPDATE catalogue_objects SET revision=revision+1,ticket_count=(SELECT count(*) FROM catalogue_edges WHERE target_id=? AND relation IN ('area','family','route')) WHERE id=? AND ticket_count<>(SELECT count(*) FROM catalogue_edges WHERE target_id=? AND relation IN ('area','family','route'))")?.execute(params![id,id,id])?;
        conn.prepare_cached("UPDATE catalogue_objects SET active=0,source_count=0,revision=revision+1 WHERE id=? AND active=1 AND NOT EXISTS(SELECT 1 FROM catalogue_sources WHERE object_id=?)")?.execute(params![id,id])?;
        conn.prepare_cached("INSERT OR IGNORE INTO catalogue_events SELECT ?,id,id,'removed',noc FROM catalogue_objects WHERE id=? AND active=0")?.execute(params![generation,id])?;
    }
    conn.execute("UPDATE catalogue_events SET reason=CASE WHEN (SELECT active FROM catalogue_objects WHERE id=previous_id)=1 THEN 'split' ELSE 'merge' END WHERE generation=? AND reason='repartition'",[generation])?;
    conn.execute(
        "UPDATE dataset SET catalogue_version=? WHERE id=1 AND catalogue_version<>?",
        params![VERSION, VERSION],
    )?;
    Ok(())
}
