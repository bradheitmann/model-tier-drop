//! Registry facts from a pinned `model-atlas.export/1` file (amendment 2026-11-01 and addendum
//! 2026-11-02, S-12 and S-13).
//!
//! model-atlas owns the model facts; tier-drop owns the tiers. This module is the pure mapping
//! between them: it reads the fields tier-drop's registry needs (`context_window`, `open_weight`,
//! `reasoning_levels`, `lineage`, `cost`) out of one pinned export and returns, for each one,
//! either a `Known` fact with its provenance or an `Unknown` fact with model-atlas's own reason.
//!
//! It reads no file, calls no model and touches no network: the caller parses the pinned bytes and
//! hands them in. The rule that matters most is that an `unknown` fact never becomes zero — an
//! unpriced route reported as `0.0` would rank first in a plan.
//!
//! The addendum of 2026-11-02 (export `2026.11.01-4`) adds three readings:
//!
//! - **Routes.** A card's prices live on its routes, matched by `route.card`; each route resolves to
//!   a [`RouteCost`] carrying its id, its cost basis (`list`, `implied` or `actual`) and its free
//!   flag. A route with no card resolves by its id alone.
//! - **Implied prices.** A route whose `cost_basis` is `implied` and whose prices are both `0` is
//!   the issue `atlas_implied_price_zero` (model-atlas D-A): a free route's implied price is not a
//!   real zero, so the cost stays `Unknown`.
//! - **Lineage.** `lineage.maker_id` is the family and is a bare slug, so the maker resolves
//!   `Known(<slug>)` even when `lineage.architecture` is unknown; the architecture is its own
//!   fact, and [`ResolvedLineage::key`] exists only when both halves are known.
//!
//! Two further decisions shape the reading:
//!
//! - The export schema grows inside `model-atlas.export/1` (v1.2, v1.3 and v1.4 each added
//!   fields), so these types do not deny unknown fields: an unreadable new field must not make a
//!   pinned export unresolvable. The fields tier-drop reads are read explicitly and checked.
//! - A card or route parsed out of an export remembers which source ids that export resolves, so
//!   the reading can report a `Known` fact whose source is not in the file. That is why the record
//!   types carry `known_sources`, which is filled by [`Export`] and never serialized.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use tier_drop_core::Issue;

/// The export schema this module reads.
pub const EXPORT_SCHEMA: &str = "model-atlas.export/1";

fn issue(code: &str, message: String) -> Issue {
    Issue {
        code: code.to_string(),
        message,
    }
}

/// One `model-atlas.export/1` file.
#[derive(Debug, Clone, Deserialize)]
pub struct Export {
    /// The schema id; `model-atlas.export/1`.
    pub schema: String,
    /// `YYYY.MM.DD-<n>`; the version a registry cites as `model-atlas:<id>@<export_version>`.
    pub export_version: String,
    /// When the export was written.
    #[serde(default)]
    pub generated_at: Option<String>,
    /// The instant the facts were read as of.
    #[serde(default)]
    pub as_of: Option<String>,
    /// Exported records, kept as JSON because this module reads only cards and routes.
    #[serde(default)]
    pub records: Vec<Value>,
    /// The sources every fact cites, so a fact resolves from the file alone.
    #[serde(default)]
    pub sources: Vec<SourceEntry>,
}

/// One entry of the export's `sources` list.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceEntry {
    /// The id every fact cites as its `source`.
    pub id: String,
    /// `url`, `evidence`, `repo` or `owner-statement`.
    #[serde(default)]
    pub kind: Option<String>,
    /// `public` or `private`; a private source carries no locator.
    #[serde(default)]
    pub visibility: Option<String>,
}

impl Export {
    /// Parse an export from its bytes. The caller reads the pinned file; this function does not.
    pub fn parse(text: &str) -> Result<Export, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// The source ids the export resolves.
    pub fn known_sources(&self) -> BTreeSet<String> {
        self.sources.iter().map(|s| s.id.clone()).collect()
    }

    /// Every card record, each carrying the export's source ids and its own routes.
    // decision: a `card` record that does not parse as a card is skipped rather than guessed at;
    // the pinned-file test asserts the card count, so a broken pin is caught there.
    pub fn cards(&self) -> Vec<Card> {
        let sources = self.known_sources();
        let routes = self.routes();
        self.records
            .iter()
            .filter(|r| r["kind"] == "card")
            .filter_map(|r| serde_json::from_value::<Card>(r.clone()).ok())
            .map(|mut c| {
                c.routes = routes
                    .iter()
                    .filter(|r| r.card.as_deref() == Some(c.id.as_str()))
                    .cloned()
                    .collect();
                c.known_sources = sources.clone();
                c
            })
            .collect()
    }

    /// Every route record, each carrying the export's source ids.
    pub fn routes(&self) -> Vec<Route> {
        let sources = self.known_sources();
        self.records
            .iter()
            .filter(|r| r["kind"] == "route")
            .filter_map(|r| serde_json::from_value::<Route>(r.clone()).ok())
            .map(|mut r| {
                r.known_sources = sources.clone();
                r
            })
            .collect()
    }

    /// The card with this id, or None.
    pub fn card(&self, id: &str) -> Option<Card> {
        self.cards().into_iter().find(|c| c.id == id)
    }

    /// The route with this id, or None.
    pub fn route(&self, id: &str) -> Option<Route> {
        self.routes().into_iter().find(|r| r.id == id)
    }

    /// Every route that names this card, in export order.
    pub fn routes_for(&self, card_id: &str) -> Vec<Route> {
        self.routes()
            .into_iter()
            .filter(|r| r.card.as_deref() == Some(card_id))
            .collect()
    }

    /// One route's cost, basis and free flag, with any issue the reading raised.
    ///
    /// A route with no card is read by its id alone, which is all the export states about it.
    pub fn route_cost(&self, route_id: &str) -> Option<(RouteCost, Vec<Issue>)> {
        let route = self.route(route_id)?;
        let mut issues = Vec::new();
        let stale = route.is_stale();
        let cost = route_cost(&route, stale, &mut issues);
        Some((cost, issues))
    }
}

/// One fact of a model-atlas record: exactly one of `value` or `unknown`.
#[derive(Debug, Clone, Deserialize)]
pub struct Fact {
    /// `documented`, `measured`, `observed` or `assumed`.
    #[serde(default)]
    pub kind: Option<String>,
    /// The UTC date the fact was last verified.
    #[serde(default)]
    pub last_verified: Option<String>,
    /// The id of the `sources` entry that states the fact.
    #[serde(default)]
    pub source: Option<String>,
    /// The value, when the fact is known.
    #[serde(default)]
    pub value: Option<Value>,
    /// The reason the fact is unknown.
    #[serde(default)]
    pub unknown: Option<String>,
    /// Sample size, required by model-atlas on an `observed` fact.
    #[serde(default)]
    pub n: Option<u64>,
    /// `low`, `medium` or `high` on an `observed` fact.
    #[serde(default)]
    pub confidence: Option<String>,
    /// model-atlas's settled flag on an `observed` fact with n < 20.
    #[serde(default)]
    pub settled: Option<bool>,
}

impl Fact {
    /// Whether the fact states a value.
    pub fn has_value(&self) -> bool {
        self.value.is_some()
    }
}

/// A card's `limits` table.
#[derive(Debug, Clone, Deserialize)]
pub struct Limits {
    /// The publisher's own context window.
    #[serde(default)]
    pub context_tokens: Option<Fact>,
}

/// A card's `lineage` table.
///
/// `maker_id` is a bare slug in schema v1.4 (not a fact); `maker` and `architecture` are facts. A
/// card whose `maker_id` arrives as a fact object does not parse, and [`Export::cards`] skips it,
/// which the pinned-file card count makes visible.
#[derive(Debug, Clone, Deserialize)]
pub struct LineageFacts {
    /// The closed maker slug of schema v1.4, for example `qwen`.
    #[serde(default)]
    pub maker_id: Option<String>,
    /// The free-text maker, which stands in when the export carries no `maker_id`.
    #[serde(default)]
    pub maker: Option<Fact>,
    /// The architecture lineage (for example dense or MoE).
    #[serde(default)]
    pub architecture: Option<Fact>,
}

/// A `model-atlas.card/1` record, as far as tier-drop reads it.
#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    /// The card id, for example `qwen3.8-27b`.
    pub id: String,
    /// Whether the weights are open.
    #[serde(default)]
    pub open_weight: Option<Fact>,
    /// The reasoning levels the publisher exposes.
    #[serde(default)]
    pub reasoning_levels: Option<Fact>,
    /// The card's limits.
    #[serde(default)]
    pub limits: Option<Limits>,
    /// The card's lineage.
    #[serde(default)]
    pub lineage: Option<LineageFacts>,
    /// `true` when model-atlas exports a stale record; may be null or absent.
    #[serde(default)]
    pub stale: Option<bool>,
    /// `current` or `stale`.
    #[serde(default)]
    pub freshness: Option<String>,
    /// The source ids of the export this card came from; not part of the record.
    #[serde(skip)]
    pub known_sources: BTreeSet<String>,
    /// The routes that name this card, filled by [`Export::cards`]; not part of the record.
    #[serde(skip)]
    pub routes: Vec<Route>,
}

impl Card {
    /// Whether model-atlas marks the record stale. `null` and an absent field both mean not stale.
    pub fn is_stale(&self) -> bool {
        self.stale == Some(true) || self.freshness.as_deref() == Some("stale")
    }
}

/// A `model-atlas.route/1` record, as far as tier-drop reads it.
#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    /// For example `route.deepseek.deepseek-flash`.
    pub id: String,
    /// The card this route serves, when the export names one.
    #[serde(default)]
    pub card: Option<String>,
    /// Why no card is named, when it is not. A route is read by id either way.
    #[serde(default)]
    pub card_unknown: Option<String>,
    /// The tariff: a table of price facts with `cost_basis` in export -4, or one fact in earlier
    /// exports. An unpriced route carries `unknown`, never zeros.
    #[serde(default)]
    pub tariff: Option<Value>,
    /// Whether the route costs the caller nothing.
    #[serde(default)]
    pub is_free: Option<Fact>,
    /// The context the route accepts, which may be less than the card's limit.
    #[serde(default)]
    pub context_offered_tokens: Option<Fact>,
    /// `true` when model-atlas exports a stale record.
    #[serde(default)]
    pub stale: Option<bool>,
    /// `current` or `stale`.
    #[serde(default)]
    pub freshness: Option<String>,
    /// The source ids of the export this route came from; not part of the record.
    #[serde(skip)]
    pub known_sources: BTreeSet<String>,
}

impl Route {
    /// Whether model-atlas marks the record stale.
    pub fn is_stale(&self) -> bool {
        self.stale == Some(true) || self.freshness.as_deref() == Some("stale")
    }
}

/// How a route's price came about (model-atlas v1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasis {
    /// The provider's published price.
    List,
    /// A free route priced at the same model's paid price elsewhere. Never a cost the caller pays.
    Implied,
    /// Measured spend from telemetry.
    Actual,
}

impl CostBasis {
    // decision: a tariff without a readable `cost_basis` is read as a list price, which is what
    // every tariff in the earlier exports was; a new basis name is not guessed at.
    fn of(tariff: &Value) -> CostBasis {
        match tariff.get("cost_basis").and_then(Value::as_str) {
            Some("implied") => CostBasis::Implied,
            Some("actual") => CostBasis::Actual,
            _ => CostBasis::List,
        }
    }
}

/// A resolved fact: a value with its provenance, or the reason it has none.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Resolved<T> {
    /// model-atlas states the fact.
    Known {
        /// The value.
        value: T,
        /// The `sources` id that states it.
        source: String,
        /// The UTC date it was last verified.
        last_verified: String,
        /// Whether the record it came from is stale.
        stale: bool,
    },
    /// model-atlas states why the fact is unknown, or the export does not carry it.
    Unknown {
        /// model-atlas's reason, or the field that is missing.
        reason: String,
        /// Whether the record it came from is stale.
        stale: bool,
    },
}

impl<T> Resolved<T> {
    /// Whether the fact is known.
    pub fn is_known(&self) -> bool {
        matches!(self, Resolved::Known { .. })
    }

    /// The value, when known.
    pub fn value(&self) -> Option<&T> {
        match self {
            Resolved::Known { value, .. } => Some(value),
            Resolved::Unknown { .. } => None,
        }
    }

    /// The reason, when unknown.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Resolved::Known { .. } => None,
            Resolved::Unknown { reason, .. } => Some(reason),
        }
    }

    /// The source id, when known.
    pub fn source(&self) -> Option<&str> {
        match self {
            Resolved::Known { source, .. } => Some(source),
            Resolved::Unknown { .. } => None,
        }
    }

    /// The last-verified date, when known.
    pub fn last_verified(&self) -> Option<&str> {
        match self {
            Resolved::Known { last_verified, .. } => Some(last_verified),
            Resolved::Unknown { .. } => None,
        }
    }

    /// Whether the record this fact came from is stale.
    pub fn stale(&self) -> bool {
        match self {
            Resolved::Known { stale, .. } | Resolved::Unknown { stale, .. } => *stale,
        }
    }
}

/// A route's tariff, as tier-drop's registry records it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cost {
    /// USD per million input tokens.
    pub input_usd_per_mtok: f64,
    /// USD per million output tokens.
    pub output_usd_per_mtok: f64,
}

/// One route's price and the two things a registry must cite beside it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteCost {
    /// The route id a registry binds as `model-atlas:<route id>@<export_version>`.
    pub route_id: String,
    /// The card the route names, when it names one.
    pub card_id: Option<String>,
    /// The price, or the reason there is none. Never zero unless the export states a real zero.
    pub cost: Resolved<Cost>,
    /// `list`, `implied` or `actual`.
    pub cost_basis: CostBasis,
    /// Whether the route costs the caller nothing.
    pub is_free: Resolved<bool>,
    /// Whether the record is stale.
    pub stale: bool,
}

/// A card's lineage, split into the two facts model-atlas keeps separate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedLineage {
    /// The maker: `lineage.maker_id` when the export carries it, else the free-text maker.
    pub maker: Resolved<String>,
    /// The architecture lineage, which the export may leave unknown.
    pub architecture: Resolved<String>,
    /// Whether `maker` is the free-text `lineage.maker` rather than the closed `maker_id` slug.
    pub maker_free_text: bool,
}

impl ResolvedLineage {
    /// Whether both halves are known.
    pub fn is_known(&self) -> bool {
        self.maker.is_known() && self.architecture.is_known()
    }

    /// The identity tier-drop compares for "same tier, other lineage", when both halves are known.
    // decision: an unknown architecture yields no key rather than a maker-only key, so `adjacent`
    // never treats two differently-architected models as one family.
    pub fn key(&self) -> Option<String> {
        match (self.maker.value(), self.architecture.value()) {
            (Some(maker), Some(architecture)) => Some(format!("{maker}/{architecture}")),
            _ => None,
        }
    }

    /// Whether either half comes from a stale record.
    pub fn stale(&self) -> bool {
        self.maker.stale() || self.architecture.stale()
    }
}

/// The registry facts of one card, each Known or Unknown, with the issues found on the way.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedFacts {
    /// The card these facts belong to.
    pub card_id: String,
    /// The route they were read through, when the caller named one.
    pub route_id: Option<String>,
    /// Whether the record is stale (model-atlas G2); every fact carries the same flag.
    pub stale: bool,
    /// The context window: the named route's offered context, else the card's own limit.
    pub context_window: Resolved<u64>,
    /// Whether the weights are open.
    pub open_weight: Resolved<bool>,
    /// The reasoning levels the publisher exposes.
    pub reasoning_levels: Resolved<Vec<String>>,
    /// The lineage, split into maker and architecture.
    pub lineage: ResolvedLineage,
    /// The cost through the named route, or through the card's only route. Never zero unless stated.
    pub cost: Resolved<Cost>,
    /// Every route the export gives for this card, each with its basis and free flag.
    pub routes: Vec<RouteCost>,
    /// `atlas_fact_missing`, `atlas_source_unresolved` and `atlas_implied_price_zero` findings;
    /// empty on a clean read.
    pub issues: Vec<Issue>,
}

/// Read a fact as `T`, reporting a missing fact and an unresolvable source.
///
/// The value is never coerced: when model-atlas states a value of a shape a registry field cannot
/// hold, the fact resolves to Unknown with the observed value in the reason.
fn resolve_fact<T>(
    record_id: &str,
    field: &str,
    fact: Option<&Fact>,
    known_sources: &BTreeSet<String>,
    stale: bool,
    issues: &mut Vec<Issue>,
    read: impl Fn(&Value) -> Option<T>,
) -> Resolved<T> {
    let Some(fact) = fact else {
        issues.push(issue(
            "atlas_fact_missing",
            format!("record `{record_id}` has no `{field}` fact"),
        ));
        return Resolved::Unknown {
            reason: format!("fact missing from the export: {field}"),
            stale,
        };
    };
    let Some(value) = &fact.value else {
        if let Some(reason) = &fact.unknown {
            return Resolved::Unknown {
                reason: reason.clone(),
                stale,
            };
        }
        issues.push(issue(
            "atlas_fact_missing",
            format!("record `{record_id}` fact `{field}` states neither a value nor a reason"),
        ));
        return Resolved::Unknown {
            reason: format!("fact missing from the export: {field}"),
            stale,
        };
    };
    let source = match fact.source.as_deref() {
        Some(source) => source.to_string(),
        None => {
            issues.push(issue(
                "atlas_source_unresolved",
                format!("record `{record_id}` fact `{field}` cites no source"),
            ));
            String::new()
        }
    };
    // decision: a Known fact must name a source the export carries; the fact still resolves, so a
    // reader sees the value and the finding side by side rather than a silent value with no origin.
    if !source.is_empty() && !known_sources.contains(&source) {
        issues.push(issue(
            "atlas_source_unresolved",
            format!(
                "record `{record_id}` fact `{field}` cites source `{source}`, which the export does not carry"
            ),
        ));
    }
    match read(value) {
        Some(value) => Resolved::Known {
            value,
            source,
            last_verified: fact.last_verified.clone().unwrap_or_default(),
            stale,
        },
        None => Resolved::Unknown {
            reason: format!("`{field}` has a value tier-drop cannot read: {value}"),
            stale,
        },
    }
}

fn read_bool(v: &Value) -> Option<bool> {
    v.as_bool()
}

fn read_u64(v: &Value) -> Option<u64> {
    v.as_u64()
}

fn read_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn read_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn read_string_list(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

/// A tariff stated as one fact's value (`{input_usd_per_mtok, output_usd_per_mtok}`).
fn read_cost_value(v: &Value) -> Option<Cost> {
    let input = v.get("input_usd_per_mtok")?.as_f64()?;
    let output = v.get("output_usd_per_mtok")?.as_f64()?;
    Some(Cost {
        input_usd_per_mtok: input,
        output_usd_per_mtok: output,
    })
}

/// Parse a JSON value as a fact object. Every `Fact` field is optional, so this succeeds for any
/// object; it is only called where the export states a fact.
fn fact(value: &Value) -> Fact {
    serde_json::from_value(value.clone()).unwrap_or(Fact {
        kind: None,
        last_verified: None,
        source: None,
        value: None,
        unknown: None,
        n: None,
        confidence: None,
        settled: None,
    })
}

/// Refuse an implied price of `0/0` (model-atlas D-A): a free route's implied price is not a real
/// zero, so it stays Unknown with a reason rather than ranking first in a plan.
fn refuse_implied_zero(
    route_id: &str,
    cost: Resolved<Cost>,
    basis: CostBasis,
    stale: bool,
    issues: &mut Vec<Issue>,
) -> Resolved<Cost> {
    if basis != CostBasis::Implied {
        return cost;
    }
    let Some(value) = cost.value() else {
        return cost;
    };
    if value.input_usd_per_mtok != 0.0 || value.output_usd_per_mtok != 0.0 {
        return cost;
    }
    issues.push(issue(
        "atlas_implied_price_zero",
        format!(
            "route `{route_id}` carries an implied price of 0/0; model-atlas D-A rejects it, so the price stays unknown"
        ),
    ));
    Resolved::Unknown {
        reason: "an implied price of 0 is not a real zero (model-atlas D-A)".to_string(),
        stale,
    }
}

/// One route's price, from either tariff shape, with the implied-zero rule applied.
fn route_cost(route: &Route, stale: bool, issues: &mut Vec<Issue>) -> RouteCost {
    let basis = route
        .tariff
        .as_ref()
        .map(CostBasis::of)
        .unwrap_or(CostBasis::List);
    let cost = match route.tariff.as_ref() {
        None => {
            issues.push(issue(
                "atlas_fact_missing",
                format!("record `{}` has no `tariff` fact", route.id),
            ));
            Resolved::Unknown {
                reason: "fact missing from the export: tariff".to_string(),
                stale,
            }
        }
        // Export -4: the tariff is a table whose price members are facts.
        Some(tariff)
            if tariff.get("input_usd_per_mtok").is_some()
                || tariff.get("output_usd_per_mtok").is_some() =>
        {
            let input = tariff.get("input_usd_per_mtok").map(fact);
            let output = tariff.get("output_usd_per_mtok").map(fact);
            let input = resolve_fact(
                &route.id,
                "tariff.input_usd_per_mtok",
                input.as_ref(),
                &route.known_sources,
                stale,
                issues,
                read_f64,
            );
            let output = resolve_fact(
                &route.id,
                "tariff.output_usd_per_mtok",
                output.as_ref(),
                &route.known_sources,
                stale,
                issues,
                read_f64,
            );
            match (input, output) {
                (
                    Resolved::Known {
                        value: input,
                        source,
                        last_verified,
                        ..
                    },
                    Resolved::Known {
                        value: output,
                        last_verified: output_verified,
                        ..
                    },
                ) => Resolved::Known {
                    value: Cost {
                        input_usd_per_mtok: input,
                        output_usd_per_mtok: output,
                    },
                    // decision: the price is as fresh as its older half, and the input side's
                    // source is the route's primary provenance; both are checked above.
                    last_verified: last_verified.min(output_verified),
                    source,
                    stale,
                },
                (input, output) => {
                    let reason = join_distinct_reasons([input.reason(), output.reason()]);
                    Resolved::Unknown {
                        reason: if reason.is_empty() {
                            "the route states no price".to_string()
                        } else {
                            reason
                        },
                        stale,
                    }
                }
            }
        }
        // Earlier exports: the whole tariff is one fact.
        Some(tariff) => {
            let tariff = fact(tariff);
            resolve_fact(
                &route.id,
                "tariff",
                Some(&tariff),
                &route.known_sources,
                stale,
                issues,
                read_cost_value,
            )
        }
    };
    let cost = refuse_implied_zero(&route.id, cost, basis, stale, issues);
    let is_free = resolve_fact(
        &route.id,
        "is_free",
        route.is_free.as_ref(),
        &route.known_sources,
        stale,
        issues,
        read_bool,
    );
    RouteCost {
        route_id: route.id.clone(),
        card_id: route.card.clone(),
        cost,
        cost_basis: basis,
        is_free,
        stale,
    }
}

/// Join the reasons of the two price members, keeping each distinct reason once.
///
/// decision: a reason that is identical to, or contained in, a reason already kept is dropped.
/// The glm-5.3-flash route states the same Coding Plan `unknown` on its input, output and
/// cached-input members, so a plain join states the Coding Plan clause twice and reads as two
/// findings where the export has one. On that route the input reason is the fuller of the two, so
/// the kept reason loses nothing; the order is the export's own input-then-output order.
fn join_distinct_reasons<'a>(reasons: impl IntoIterator<Item = Option<&'a str>>) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for reason in reasons.into_iter().flatten() {
        if kept
            .iter()
            .any(|already| *already == reason || already.contains(reason))
        {
            continue;
        }
        kept.push(reason);
    }
    kept.join("; ")
}

/// The card's lineage: `maker_id` is the family, and the architecture is its own fact.
fn resolve_lineage(card: &Card, stale: bool, issues: &mut Vec<Issue>) -> ResolvedLineage {
    let Some(facts) = card.lineage.as_ref() else {
        issues.push(issue(
            "atlas_fact_missing",
            format!("record `{}` has no `lineage` table", card.id),
        ));
        let reason = "fact missing from the export: lineage".to_string();
        return ResolvedLineage {
            maker: Resolved::Unknown {
                reason: reason.clone(),
                stale,
            },
            architecture: Resolved::Unknown { reason, stale },
            maker_free_text: false,
        };
    };
    let maker_free_text = facts.maker_id.is_none();
    let maker = match (facts.maker_id.as_deref(), facts.maker.as_ref()) {
        // The slug is the family. Its provenance is the `lineage.maker` fact when the export states
        // one, so the registry can cite where the maker came from.
        (Some(slug), stated) => {
            let provenance = stated
                .map(|f| {
                    resolve_fact(
                        &card.id,
                        "lineage.maker",
                        Some(f),
                        &card.known_sources,
                        stale,
                        issues,
                        read_string,
                    )
                })
                .and_then(|r| match r {
                    Resolved::Known {
                        source,
                        last_verified,
                        ..
                    } => Some((source, last_verified)),
                    Resolved::Unknown { .. } => None,
                });
            let (source, last_verified) = provenance.unwrap_or_default();
            Resolved::Known {
                value: slug.to_string(),
                source,
                last_verified,
                stale,
            }
        }
        (None, stated) => resolve_fact(
            &card.id,
            "lineage.maker",
            stated,
            &card.known_sources,
            stale,
            issues,
            read_string,
        ),
    };
    let architecture = resolve_fact(
        &card.id,
        "lineage.architecture",
        facts.architecture.as_ref(),
        &card.known_sources,
        stale,
        issues,
        read_string,
    );
    ResolvedLineage {
        maker,
        architecture,
        maker_free_text,
    }
}

/// The registry facts of `card`, read through `route` when the caller names one.
///
/// A card's routes are always returned in `routes`, each with its own price, basis and free flag.
/// The single `cost` is the named route's price, or the price of the card's only route; a card with
/// no route states `Unknown("no route in export")` and a card with several states that one must be
/// named. Without a route the context window falls back to the card's own limit. An `unknown` fact
/// is always reported as Unknown with its reason: an unpriced tariff never becomes `0.0`.
pub fn resolve(card: &Card, route: Option<&Route>) -> ResolvedFacts {
    let stale = card.is_stale()
        || route.is_some_and(Route::is_stale)
        || card.routes.iter().any(Route::is_stale);
    let mut issues = Vec::new();

    let mut routes: Vec<RouteCost> = Vec::new();
    for r in &card.routes {
        let stale_route = stale || r.is_stale();
        routes.push(route_cost(r, stale_route, &mut issues));
    }
    if let Some(named) = route {
        if !routes.iter().any(|rc| rc.route_id == named.id) {
            let stale_route = stale || named.is_stale();
            routes.push(route_cost(named, stale_route, &mut issues));
        }
    }

    let context_window = match route {
        Some(route) => resolve_fact(
            &route.id,
            "context_offered_tokens",
            route.context_offered_tokens.as_ref(),
            &route.known_sources,
            stale,
            &mut issues,
            read_u64,
        ),
        None => resolve_fact(
            &card.id,
            "limits.context_tokens",
            card.limits.as_ref().and_then(|l| l.context_tokens.as_ref()),
            &card.known_sources,
            stale,
            &mut issues,
            read_u64,
        ),
    };
    let open_weight = resolve_fact(
        &card.id,
        "open_weight",
        card.open_weight.as_ref(),
        &card.known_sources,
        stale,
        &mut issues,
        read_bool,
    );
    let reasoning_levels = resolve_fact(
        &card.id,
        "reasoning_levels",
        card.reasoning_levels.as_ref(),
        &card.known_sources,
        stale,
        &mut issues,
        read_string_list,
    );
    let lineage = resolve_lineage(card, stale, &mut issues);
    let cost = match route {
        Some(named) => routes
            .iter()
            .find(|rc| rc.route_id == named.id)
            .map(|rc| rc.cost.clone())
            .unwrap_or(Resolved::Unknown {
                reason: "no route in export".to_string(),
                stale,
            }),
        None => match routes.as_slice() {
            [] => Resolved::Unknown {
                reason: "no route in export".to_string(),
                stale,
            },
            [only] => only.cost.clone(),
            _ => Resolved::Unknown {
                reason: "several routes: pass one to resolve".to_string(),
                stale,
            },
        },
    };

    ResolvedFacts {
        card_id: card.id.clone(),
        route_id: route.map(|r| r.id.clone()),
        stale,
        context_window,
        open_weight,
        reasoning_levels,
        lineage,
        cost,
        routes,
        issues,
    }
}
