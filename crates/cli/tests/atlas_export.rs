//! S-12/T-25: the pinned model-atlas export mapped to tier-drop registry facts.
//!
//! Fixture: `crates/cli/tests/fixtures/model-atlas-export-2026.11.01-2.json`, vendored byte for
//! byte: `model-atlas.export/1`, export_version `2026.11.01-2`, sha256
//! `e126284754e2404e16fb05712da1eb14a43dc2637379678bf0b5ab627ac3e59a`, produced by model-atlas
//! commit `33477fc2eff0b415cb751c6999b2f008c05aa340`, landed on model-atlas main `8519e20` at
//! `exports/model-atlas.export-2026.11.01-2.json`.
//!
//! Export #1 has one card, three providers, no routes and nothing stale, so the unpriced-route and
//! stale-record paths are exercised on DERIVED copies: the pinned bytes are parsed, one value is
//! changed, and the copy is re-serialized in memory. No file is written and the pinned file, with
//! its sha256, is never touched.

use serde_json::{json, Value};
use tier_drop_cli::atlas::{resolve, Export};

const FIXTURE: &str = include_str!("fixtures/model-atlas-export-2026.11.01-2.json");
const PINNED_SHA256: &str = "e126284754e2404e16fb05712da1eb14a43dc2637379678bf0b5ab627ac3e59a";
const PINNED_VERSION: &str = "2026.11.01-2";
const CARD_ID: &str = "qwen3.8-27b";

fn sha256(bytes: &[u8]) -> String {
    tier_drop_core::sha256_hex(bytes)
}

fn pinned() -> Export {
    Export::parse(FIXTURE).expect("the pinned export parses")
}

/// The pinned export as a mutable JSON value.
fn pinned_value() -> Value {
    serde_json::from_str(FIXTURE).expect("the pinned export is JSON")
}

/// A derived copy: `edit` changes one part of the value, which is re-serialized in memory.
fn derived(edit: impl FnOnce(&mut Value)) -> Export {
    let mut value = pinned_value();
    edit(&mut value);
    let text = serde_json::to_string(&value).expect("the derived copy serializes");
    Export::parse(&text).expect("the derived copy parses")
}

/// The index of the card record in a derived value.
fn card_index(value: &Value) -> usize {
    value["records"]
        .as_array()
        .expect("records is a list")
        .iter()
        .position(|r| r["kind"] == "card")
        .expect("the export holds a card")
}

/// A route record for the pinned card, as model-atlas writes one (`model-atlas.route/1`).
fn route_record(tariff: Value) -> Value {
    json!({
        "id": "route.openrouter.qwen3.8-27b",
        "kind": "route",
        "card": CARD_ID,
        "provider": "provider.openrouter",
        "provider_model_id": {
            "kind": "documented",
            "last_verified": "2026-10-31",
            "source": "src.field-window-refresh-1",
            "value": "qwen/qwen3.8-27b"
        },
        "is_free": {
            "kind": "documented",
            "last_verified": "2026-10-31",
            "source": "src.field-window-refresh-1",
            "value": false
        },
        "tariff": tariff,
        "context_offered_tokens": {
            "kind": "documented",
            "last_verified": "2026-10-31",
            "source": "src.field-window-refresh-1",
            "value": 262144
        },
        "freshness_days": 7
    })
}

#[test]
fn atlas_fixture_matches_pinned_sha256() {
    assert_eq!(
        sha256(FIXTURE.as_bytes()),
        PINNED_SHA256,
        "the vendored fixture is not the pinned export"
    );
    let export = pinned();
    assert_eq!(export.schema, "model-atlas.export/1");
    assert_eq!(export.export_version, PINNED_VERSION);
    assert_eq!(export.cards().len(), 1, "export #1 holds one card");
    assert_eq!(export.routes().len(), 0, "export #1 carries no route");

    // One changed character of a fact value changes the digest, so the pin catches a hand edit.
    let mut edited = pinned_value();
    let i = card_index(&edited);
    edited["records"][i]["open_weight"]["value"] = json!(false);
    let bytes = serde_json::to_string(&edited).unwrap();
    assert_ne!(sha256(bytes.as_bytes()), PINNED_SHA256);
}

#[test]
fn atlas_card_resolves_without_missing_facts() {
    let export = pinned();
    let card = export
        .card(CARD_ID)
        .expect("the pinned export holds the card");
    let facts = resolve(&card, None);
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);
    assert_eq!(facts.card_id, CARD_ID);
    assert_eq!(facts.route_id, None);
    assert!(!facts.stale);

    // open_weight is the one fact the card states about a registry field.
    assert_eq!(facts.open_weight.value(), Some(&true));
    assert_eq!(facts.open_weight.source(), Some("src.serving-roster"));
    assert_eq!(facts.open_weight.last_verified(), Some("2026-10-31"));
    assert!(!facts.open_weight.stale());

    // The rest are Unknown, carrying model-atlas's own reasons rather than an invented value.
    assert_eq!(facts.context_window.value(), None);
    assert!(facts
        .context_window
        .reason()
        .is_some_and(|r| r.contains("context window")));
    assert_eq!(facts.reasoning_levels.value(), None);
    assert!(facts
        .reasoning_levels
        .reason()
        .is_some_and(|r| r.contains("reasoning levels")));
    assert_eq!(facts.lineage.value(), None);
    assert!(facts
        .lineage
        .reason()
        .is_some_and(|r| r.contains("maker") && r.contains("architecture")));
    for reason in [
        facts.context_window.reason(),
        facts.reasoning_levels.reason(),
        facts.lineage.reason(),
    ] {
        assert!(
            reason.is_some_and(|r| !r.is_empty()),
            "an Unknown fact always gives a reason"
        );
    }
    // A record with no `stale` field, or with `stale: null`, is not stale.
    assert!(!facts.open_weight.stale());
    let null_stale = derived(|v| {
        let i = card_index(v);
        v["records"][i]["stale"] = Value::Null;
    });
    let facts = resolve(&null_stale.card(CARD_ID).unwrap(), None);
    assert!(!facts.stale);
    assert!(!facts.open_weight.stale());

    // A fact removed from a copy is an issue naming the record and the field, never a silent gap.
    let missing = derived(|v| {
        let i = card_index(v);
        v["records"][i]
            .as_object_mut()
            .expect("a card is an object")
            .remove("open_weight");
    });
    let facts = resolve(
        &missing.card(CARD_ID).expect("the copy holds the card"),
        None,
    );
    let issue = facts
        .issues
        .iter()
        .find(|i| i.code == "atlas_fact_missing")
        .unwrap_or_else(|| panic!("no atlas_fact_missing issue: {:?}", facts.issues));
    assert!(issue.message.contains(CARD_ID), "{}", issue.message);
    assert!(issue.message.contains("open_weight"), "{}", issue.message);
    assert_eq!(facts.open_weight.value(), None);
    assert!(facts.open_weight.reason().is_some());
}

#[test]
fn atlas_card_without_route_cost_is_unknown() {
    let export = pinned();
    let card = export
        .card(CARD_ID)
        .expect("the pinned export holds the card");
    let facts = resolve(&card, None);
    // No route: the cost is Unknown with a stated reason, and never 0.0.
    assert_eq!(facts.cost.value(), None);
    assert_eq!(facts.cost.reason(), Some("no route in export"));
    let text = serde_json::to_string(&facts.cost).expect("the fact serializes");
    assert!(text.contains("unknown"), "{text}");
    assert!(
        !text.contains("0.0"),
        "an unpriced fact is never zero: {text}"
    );
    // The context window falls back to the card's own fact, which this export marks unknown.
    assert!(facts
        .context_window
        .reason()
        .is_some_and(|r| r.contains("context window")));
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);
}

#[test]
fn atlas_unpriced_route_cost_stays_unknown() {
    let export = derived(|v| {
        let route = route_record(json!({
            "kind": "documented",
            "last_verified": "2026-10-31",
            "source": "src.field-window-refresh-1",
            "unknown": "the provider's price list was not read in this slice"
        }));
        v["records"].as_array_mut().unwrap().push(route);
    });
    let card = export
        .card(CARD_ID)
        .expect("the derived copy holds the card");
    let route = export
        .route("route.openrouter.qwen3.8-27b")
        .expect("the derived copy holds the route");
    let facts = resolve(&card, Some(&route));
    assert_eq!(
        facts.route_id.as_deref(),
        Some("route.openrouter.qwen3.8-27b")
    );
    // An unknown tariff stays Unknown with its reason: never 0.0, which would rank it first.
    assert_eq!(facts.cost.value(), None);
    assert!(facts
        .cost
        .reason()
        .is_some_and(|r| r.contains("price list was not read")));
    let text = serde_json::to_string(&facts.cost).unwrap();
    assert!(
        !text.contains("0.0"),
        "an unpriced tariff is never zero: {text}"
    );
    // The route's offered context is known, so it replaces the card's unknown limit.
    assert_eq!(facts.context_window.value(), Some(&262_144));
    assert_eq!(
        facts.context_window.source(),
        Some("src.field-window-refresh-1")
    );
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);
}

#[test]
fn atlas_stale_record_keeps_flag() {
    let export = derived(|v| {
        let i = card_index(v);
        v["records"][i]["stale"] = json!(true);
    });
    let card = export
        .card(CARD_ID)
        .expect("the derived copy holds the card");
    let facts = resolve(&card, None);
    // A stale record is never reported as current: every fact carries the flag.
    assert!(facts.stale, "the record is stale");
    assert!(facts.open_weight.stale());
    assert!(facts.context_window.stale());
    assert!(facts.reasoning_levels.stale());
    assert!(facts.lineage.stale());
    assert!(facts.cost.stale());
    // ... and the resolved value itself is unchanged.
    assert_eq!(facts.open_weight.value(), Some(&true));
    // The pinned card, which carries no `stale` field, is not stale.
    assert!(!resolve(&pinned().card(CARD_ID).unwrap(), None).stale);
}

#[test]
fn atlas_every_fact_source_resolves() {
    let export = pinned();
    assert!(!export.known_sources().is_empty());
    for card in export.cards() {
        let facts = resolve(&card, None);
        assert!(
            facts
                .issues
                .iter()
                .all(|i| i.code != "atlas_source_unresolved"),
            "{}: {:?}",
            card.id,
            facts.issues
        );
    }
    for route in export.routes() {
        let card = export.card(CARD_ID).expect("the route names a card");
        let facts = resolve(&card, Some(&route));
        assert!(
            facts
                .issues
                .iter()
                .all(|i| i.code != "atlas_source_unresolved"),
            "{}: {:?}",
            route.id,
            facts.issues
        );
    }

    // A copy citing a source id the export does not carry is an issue, not a silent Known fact.
    let export = derived(|v| {
        let i = card_index(v);
        v["records"][i]["open_weight"]["source"] = json!("src.absent");
    });
    let card = export
        .card(CARD_ID)
        .expect("the derived copy holds the card");
    let facts = resolve(&card, None);
    let issue = facts
        .issues
        .iter()
        .find(|i| i.code == "atlas_source_unresolved")
        .unwrap_or_else(|| panic!("no atlas_source_unresolved issue: {:?}", facts.issues));
    assert!(issue.message.contains(CARD_ID), "{}", issue.message);
    assert!(issue.message.contains("src.absent"), "{}", issue.message);
    assert!(facts.open_weight.value().is_some());
}
