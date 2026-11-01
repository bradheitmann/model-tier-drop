//! Registry facts from a pinned `model-atlas.export/1` file (amendment 2026-11-01, S-12).
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
//! Two decisions shape the reading:
//!
//! - The export schema grows inside `model-atlas.export/1` (v1.2, v1.3 and v1.4 each added
//!   fields), so these types do not deny unknown fields: an unreadable new field must not make a
//!   pinned export unresolvable. The fields tier-drop reads are read explicitly and checked.
//! - A card or route parsed out of an export remembers which source ids that export resolves, so
//!   [`resolve`] can report a `Known` fact whose source is not in the file. That is why the record
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

    /// Every card record, each carrying the export's source ids.
    // decision: a `card` record that does not parse as a card is skipped rather than guessed at;
    // the pinned-file test asserts the card count, so a broken pin is caught there.
    pub fn cards(&self) -> Vec<Card> {
        let sources = self.known_sources();
        self.records
            .iter()
            .filter(|r| r["kind"] == "card")
            .filter_map(|r| serde_json::from_value::<Card>(r.clone()).ok())
            .map(|mut c| {
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

    /// Whether the fact states a reason instead of a value.
    pub fn has_unknown(&self) -> bool {
        self.value.is_none() && self.unknown.is_some()
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
#[derive(Debug, Clone, Deserialize)]
pub struct LineageFacts {
    /// The closed maker slug of schema v1.4, when the export carries it.
    #[serde(default)]
    pub maker_id: Option<Fact>,
    /// The free-text maker, which stands in until an export carries `maker_id`.
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
    /// For example `route.openrouter.qwen3.8-27b`.
    pub id: String,
    /// What the route charges; an unpriced route carries `unknown`, never zeros.
    #[serde(default)]
    pub tariff: Option<Fact>,
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

/// A card's lineage, as tier-drop's `Model.lineage` reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lineage {
    /// The maker: the v1.4 `maker_id` slug when the export carries one, else the free text.
    pub maker: String,
    /// The architecture lineage.
    pub architecture: String,
    /// Whether `maker` is the free-text `lineage.maker` rather than the closed `maker_id` slug.
    pub maker_free_text: bool,
    /// The source id that states the maker.
    pub maker_source: String,
    /// The source id that states the architecture.
    pub architecture_source: String,
}

impl Lineage {
    /// The identity tier-drop compares for "same tier, other lineage".
    pub fn key(&self) -> String {
        format!("{}/{}", self.maker, self.architecture)
    }
}

/// The registry facts of one card, each Known or Unknown, with the issues found on the way.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedFacts {
    /// The card these facts belong to.
    pub card_id: String,
    /// The route they were read through, when one was given.
    pub route_id: Option<String>,
    /// Whether the record is stale (model-atlas G2); every fact carries the same flag.
    pub stale: bool,
    /// The context window: the route's offered context, else the card's own limit.
    pub context_window: Resolved<u64>,
    /// Whether the weights are open.
    pub open_weight: Resolved<bool>,
    /// The reasoning levels the publisher exposes.
    pub reasoning_levels: Resolved<Vec<String>>,
    /// The lineage identity, `"<maker>/<architecture>"`.
    pub lineage: Resolved<Lineage>,
    /// What the route charges. Unknown, never zero, when there is no route or no price.
    pub cost: Resolved<Cost>,
    /// `atlas_fact_missing` and `atlas_source_unresolved` findings; empty on a clean read.
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

fn read_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn read_string_list(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

/// A tariff is a cost only when both prices are stated; one missing price is not a zero.
fn read_cost(v: &Value) -> Option<Cost> {
    let input = v.get("input_usd_per_mtok")?.as_f64()?;
    let output = v.get("output_usd_per_mtok")?.as_f64()?;
    Some(Cost {
        input_usd_per_mtok: input,
        output_usd_per_mtok: output,
    })
}

/// The maker of a card, preferring the closed v1.4 slug and falling back to the free text.
fn resolve_maker(
    card_id: &str,
    lineage: &LineageFacts,
    known_sources: &BTreeSet<String>,
    stale: bool,
    issues: &mut Vec<Issue>,
) -> (Resolved<String>, bool) {
    if let Some(fact) = lineage.maker_id.as_ref() {
        let maker = resolve_fact(
            card_id,
            "lineage.maker_id",
            Some(fact),
            known_sources,
            stale,
            issues,
            read_string,
        );
        if maker.is_known() {
            return (maker, false);
        }
    }
    let maker = resolve_fact(
        card_id,
        "lineage.maker",
        lineage.maker.as_ref(),
        known_sources,
        stale,
        issues,
        read_string,
    );
    (maker, true)
}

/// The registry facts of `card`, read through `route` when one is given.
///
/// A route supplies the cost and the context it offers; without a route the context falls back to
/// the card's own limit and the cost is `Unknown("no route in export")`. An `unknown` fact is
/// always reported as Unknown with its reason: an unpriced tariff never becomes `0.0`.
pub fn resolve(card: &Card, route: Option<&Route>) -> ResolvedFacts {
    let stale = card.is_stale() || route.is_some_and(Route::is_stale);
    let mut issues = Vec::new();

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
    let lineage = match card.lineage.as_ref() {
        None => {
            issues.push(issue(
                "atlas_fact_missing",
                format!("record `{}` has no `lineage` table", card.id),
            ));
            Resolved::Unknown {
                reason: "fact missing from the export: lineage".to_string(),
                stale,
            }
        }
        Some(facts) => {
            let (maker, maker_free_text) =
                resolve_maker(&card.id, facts, &card.known_sources, stale, &mut issues);
            let architecture = resolve_fact(
                &card.id,
                "lineage.architecture",
                facts.architecture.as_ref(),
                &card.known_sources,
                stale,
                &mut issues,
                read_string,
            );
            match (maker, architecture) {
                (
                    Resolved::Known {
                        value: maker,
                        source: maker_source,
                        last_verified: maker_verified,
                        ..
                    },
                    Resolved::Known {
                        value: architecture,
                        source: architecture_source,
                        last_verified: architecture_verified,
                        ..
                    },
                ) => Resolved::Known {
                    // decision: the lineage is as fresh as its older half, and it carries both
                    // source ids inside the value, because one fact cites two record fields.
                    last_verified: maker_verified.min(architecture_verified),
                    value: Lineage {
                        maker,
                        architecture,
                        maker_free_text,
                        maker_source: maker_source.clone(),
                        architecture_source,
                    },
                    source: maker_source,
                    stale,
                },
                (maker, architecture) => {
                    let reason = [maker.reason(), architecture.reason()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("; ");
                    Resolved::Unknown {
                        reason: if reason.is_empty() {
                            "lineage is unknown".to_string()
                        } else {
                            reason
                        },
                        stale,
                    }
                }
            }
        }
    };
    let cost = match route {
        None => Resolved::Unknown {
            reason: "no route in export".to_string(),
            stale,
        },
        Some(route) => resolve_fact(
            &route.id,
            "tariff",
            route.tariff.as_ref(),
            &route.known_sources,
            stale,
            &mut issues,
            read_cost,
        ),
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
        issues,
    }
}
