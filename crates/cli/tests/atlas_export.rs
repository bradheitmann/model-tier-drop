//! S-12/T-25, S-13 and seam-sync-1: the pinned model-atlas export mapped to tier-drop registry
//! facts.
//!
//! Fixture: `crates/cli/tests/fixtures/model-atlas-export-2026.11.02-3.json`, vendored byte for
//! byte: `model-atlas.export/1`, export_version `2026.11.02-3`, sha256
//! `77879250ea316ba3f203bb3c3b3899eb3eef0b42a5304449f6d8616f6447b7e7`, produced by model-atlas
//! commit `dfc979e73884d76e12bc175e2fbc695d42346666` at
//! `exports/model-atlas.export-2026.11.02-3.json`, and marked `current` in model-atlas
//! `exports/PINS.tsv`. The earlier fixtures are gone: S-13 replaced `2026.11.01-2` and
//! seam-sync-1 replaced `2026.11.01-4`, each in one commit with `git rm`.
//!
//! Export -3 carries 4 cards, 4 providers and 6 routes (one of them a free route with an implied
//! price, and one the OpenRouter route for `qwen3.8-27b`), so the route, cost-basis and maker-id
//! paths run on the pinned file. Export -3 has no unpriced route and nothing stale, so those paths
//! run on DERIVED copies: the pinned bytes are parsed, one value is changed, and the copy is
//! re-serialized in memory. No file is written and the pinned fixture, with its sha256, is never
//! touched.

use std::path::Path;

use serde_json::{json, Value};
use tier_drop_cli::atlas::{resolve, Cost, CostBasis, Export};

const FIXTURE: &str = include_str!("fixtures/model-atlas-export-2026.11.02-3.json");
const PINNED_SHA256: &str = "77879250ea316ba3f203bb3c3b3899eb3eef0b42a5304449f6d8616f6447b7e7";
const PINNED_VERSION: &str = "2026.11.02-3";
const REPLACED_FIXTURE: &str = "model-atlas-export-2026.11.01-4.json";
const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

const CARD_IDS: [&str; 4] = [
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "qwen3.8-27b",
    "xiaomi-mimo-v2.6-flash",
];
const FREE_ROUTE: &str = "route.openrouter.nvidia-nemotron-3-5-super-vl-free";
const FREE_ROUTE_WITH_CARD: &str = "route.openrouter.nvidia-nemotron-3-5-super-vl";
const LIST_ROUTE: &str = "route.deepseek.deepseek-flash";

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

/// The index of the record with this id in a derived value.
fn record_index(value: &Value, id: &str) -> usize {
    value["records"]
        .as_array()
        .expect("records is a list")
        .iter()
        .position(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no record `{id}`"))
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
    assert_eq!(export.cards().len(), 4, "export -3 holds four cards");
    assert_eq!(export.routes().len(), 6, "export -3 carries six routes");

    // The re-pin replaced the previous fixture in one commit; the old bytes are not in the tree.
    let old = Path::new(MANIFEST_DIR)
        .join("tests")
        .join("fixtures")
        .join(REPLACED_FIXTURE);
    assert!(
        !old.exists(),
        "the 2026.11.01-2 fixture must be removed with git rm, not kept beside the pin"
    );

    // One changed character of a fact value changes the digest, so the pin catches a hand edit.
    let mut edited = pinned_value();
    let i = record_index(&edited, "qwen3.8-27b");
    edited["records"][i]["open_weight"]["value"] = json!(false);
    let bytes = serde_json::to_string(&edited).unwrap();
    assert_ne!(sha256(bytes.as_bytes()), PINNED_SHA256);
}

#[test]
fn atlas_every_card_resolves() {
    let export = pinned();
    for id in CARD_IDS {
        let card = export.card(id).unwrap_or_else(|| panic!("no card `{id}`"));
        let facts = resolve(&card, None);
        assert!(
            facts.issues.iter().all(|i| i.code != "atlas_fact_missing"),
            "{id}: {:?}",
            facts.issues
        );
        assert_eq!(facts.card_id, id);
        assert!(!facts.stale, "{id} is not stale in export -3");
        // Every card states whether its weights are open, and a value always carries provenance.
        assert!(
            facts.open_weight.is_known(),
            "{id}: open_weight should be known"
        );
        const OPEN_WEIGHT_SOURCE: &str = "src.models-dev-api";
        assert_eq!(facts.open_weight.source(), Some(OPEN_WEIGHT_SOURCE));
        // Every Unknown fact states model-atlas's reason rather than an invented value.
        for reason in [
            facts.context_window.reason(),
            facts.reasoning_levels.reason(),
            facts.lineage.architecture.reason(),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                !reason.is_empty(),
                "{id}: an Unknown fact always gives a reason"
            );
        }
    }
    // The one card whose architecture the export states carries a Known architecture.
    let qwen = resolve(&export.card("qwen3.8-27b").unwrap(), None);
    assert_eq!(
        qwen.lineage.architecture.value().map(String::as_str),
        Some("Qwen3_5ForConditionalGeneration")
    );
}

#[test]
fn atlas_route_costs_carry_basis() {
    let export = pinned();

    // A listed price: the basis is `list` and the route is not free.
    let (cost, issues) = export
        .route_cost(LIST_ROUTE)
        .unwrap_or_else(|| panic!("no route `{LIST_ROUTE}`"));
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cost.route_id, LIST_ROUTE);
    assert_eq!(cost.cost_basis, CostBasis::List);
    assert_eq!(cost.is_free.value(), Some(&false));
    assert_eq!(
        cost.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.15,
            output_usd_per_mtok: 0.6,
        })
    );
    assert_eq!(cost.cost.source(), Some("src.deepseek-pricing"));

    // A free route priced from the paid route of the same model: `implied`, free, 0.15/0.5.
    let (cost, issues) = export
        .route_cost(FREE_ROUTE)
        .unwrap_or_else(|| panic!("no route `{FREE_ROUTE}`"));
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cost.cost_basis, CostBasis::Implied);
    assert_eq!(cost.is_free.value(), Some(&true));
    assert_eq!(
        cost.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.15,
            output_usd_per_mtok: 0.5,
        })
    );

    // The same card's paid route is `list`, and a route with no card still resolves by its id.
    let (paid, _) = export.route_cost(FREE_ROUTE_WITH_CARD).unwrap();
    assert_eq!(paid.cost_basis, CostBasis::List);
    assert_eq!(paid.is_free.value(), Some(&false));
    assert_eq!(paid.card_id, None, "this route names no card");
    assert!(paid.cost.is_known());

    // A card resolved through its route carries that route's cost and the route's context.
    let card = export.card("deepseek-v4-flash").unwrap();
    let route = export.route(LIST_ROUTE).unwrap();
    let facts = resolve(&card, Some(&route));
    assert_eq!(facts.routes.len(), 1);
    assert_eq!(facts.routes[0].cost_basis, CostBasis::List);
    assert_eq!(facts.cost.value(), facts.routes[0].cost.value());
    assert_eq!(facts.context_window.value(), Some(&1_048_576));
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);
}

/// seam-sync-1: the OpenRouter route for `qwen3.8-27b`, new in export 2026.11.02-3, resolves for
/// its card through the card's own route list, with the basis and free flag the export states.
#[test]
fn atlas_qwen_route_cost_resolves() {
    const QWEN_ROUTE: &str = "route.openrouter.qwen-qwen3-8-27b";
    const QWEN_CARD: &str = "qwen3.8-27b";
    let export = pinned();

    // Read by route id: a list price of 0.15 in and 1.875 out, not free.
    let (cost, issues) = export
        .route_cost(QWEN_ROUTE)
        .unwrap_or_else(|| panic!("no route `{QWEN_ROUTE}`"));
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cost.card_id.as_deref(), Some(QWEN_CARD));
    assert_eq!(cost.cost_basis, CostBasis::List);
    assert_eq!(cost.is_free.value(), Some(&false));
    assert_eq!(
        cost.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.15,
            output_usd_per_mtok: 1.875,
        })
    );
    assert_eq!(cost.cost.source(), Some("src.openrouter-qwen-endpoints"));

    // Read through the card: the route is matched by `route.card`, so it appears in the card's own
    // route list and prices the card's single-route cost.
    let card = export
        .card(QWEN_CARD)
        .expect("the pinned export holds the card");
    let facts = resolve(&card, None);
    assert_eq!(facts.routes.len(), 1, "{:?}", facts.routes);
    assert_eq!(facts.routes[0].route_id, QWEN_ROUTE);
    assert_eq!(facts.routes[0].cost_basis, CostBasis::List);
    assert_eq!(facts.routes[0].is_free.value(), Some(&false));
    assert_eq!(facts.cost.value(), cost.cost.value());
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);
}

#[test]
fn atlas_implied_price_zero_is_an_issue() {
    // model-atlas D-A, fixed in -4: an implied price of 0 is not a real zero.
    let export = derived(|v| {
        let i = record_index(v, FREE_ROUTE);
        v["records"][i]["tariff"]["input_usd_per_mtok"]["value"] = json!(0.0);
        v["records"][i]["tariff"]["output_usd_per_mtok"]["value"] = json!(0.0);
    });
    let (cost, issues) = export
        .route_cost(FREE_ROUTE)
        .unwrap_or_else(|| panic!("no route `{FREE_ROUTE}`"));
    assert!(
        issues.iter().any(|i| i.code == "atlas_implied_price_zero"),
        "{issues:?}"
    );
    assert_eq!(cost.cost_basis, CostBasis::Implied);
    assert_ne!(
        cost.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.0,
            output_usd_per_mtok: 0.0,
        }),
        "an implied 0 must never be taken as a real price"
    );
    assert!(
        cost.cost.reason().is_some(),
        "the cost is Unknown with a reason"
    );
}

#[test]
fn atlas_lineage_from_maker_id() {
    let export = pinned();
    let expected = [
        ("deepseek-v4-flash", "deepseek"),
        ("deepseek-v4-pro", "deepseek"),
        ("qwen3.8-27b", "qwen"),
        ("xiaomi-mimo-v2.6-flash", "xiaomi"),
    ];
    for (card_id, maker) in expected {
        let facts = resolve(&export.card(card_id).unwrap(), None);
        // `lineage.maker_id` is the family: a bare slug in schema v1.4, so the fact is Known.
        assert_eq!(
            facts.lineage.maker.value().map(String::as_str),
            Some(maker),
            "{card_id}"
        );
        assert!(
            !facts.lineage.maker_free_text,
            "{card_id}: maker_id is not free text"
        );
        assert!(
            facts.lineage.maker.source().is_some(),
            "{card_id}: a Known maker carries its source"
        );
    }
    // The architecture is a separate fact; where model-atlas states none, it stays Unknown with
    // the reason, and the lineage key is unavailable rather than invented.
    for card_id in [
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "xiaomi-mimo-v2.6-flash",
    ] {
        let facts = resolve(&export.card(card_id).unwrap(), None);
        assert_eq!(facts.lineage.architecture.value(), None, "{card_id}");
        assert!(
            facts
                .lineage
                .architecture
                .reason()
                .is_some_and(|r| r.contains("architecture lineage")),
            "{card_id}: {:?}",
            facts.lineage.architecture
        );
        assert_eq!(facts.lineage.key(), None, "{card_id}");
    }
    // Where both are known, the key is "<maker_id>/<architecture>".
    let qwen = resolve(&export.card("qwen3.8-27b").unwrap(), None);
    assert_eq!(
        qwen.lineage.key().as_deref(),
        Some("qwen/Qwen3_5ForConditionalGeneration")
    );
}

#[test]
fn atlas_unpriced_route_cost_stays_unknown() {
    // A derived copy whose route carries no price at all: Unknown with its reason, never 0.0.
    let export = derived(|v| {
        let i = record_index(v, LIST_ROUTE);
        v["records"][i]["tariff"] = json!({
            "kind": "documented",
            "last_verified": "2026-11-01",
            "source": "src.deepseek-pricing",
            "unknown": "the provider's price list was not read in this slice"
        });
    });
    let (cost, issues) = export.route_cost(LIST_ROUTE).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cost.cost.value(), None);
    assert!(cost
        .cost
        .reason()
        .is_some_and(|r| r.contains("price list was not read")));
    let text = serde_json::to_string(&cost.cost).unwrap();
    assert!(
        !text.contains("0.0"),
        "an unpriced route is never zero: {text}"
    );

    // An unpriced price member (the table shape of the 2026.11 exports) is the same rule.
    let export = derived(|v| {
        let i = record_index(v, LIST_ROUTE);
        v["records"][i]["tariff"]["output_usd_per_mtok"] = json!({
            "kind": "documented",
            "last_verified": "2026-11-01",
            "source": "src.deepseek-pricing",
            "unknown": "the output price was not listed for this route"
        });
    });
    let (cost, _) = export.route_cost(LIST_ROUTE).unwrap();
    assert_eq!(cost.cost.value(), None);
    assert!(cost
        .cost
        .reason()
        .is_some_and(|r| r.contains("output price was not listed")));
}

#[test]
fn atlas_card_without_route_cost_is_unknown() {
    // The pinned cards all have a route; a copy with the card's route removed states no price.
    let export = derived(|v| {
        let i = record_index(v, LIST_ROUTE);
        v["records"][i]["card"] = Value::Null;
        v["records"][i]["card_unknown"] = json!("no card for this route in the derived copy");
    });
    let facts = resolve(&export.card("deepseek-v4-flash").unwrap(), None);
    assert!(facts.routes.is_empty(), "{:?}", facts.routes);
    assert_eq!(facts.cost.value(), None);
    assert_eq!(facts.cost.reason(), Some("no route in export"));
    let text = serde_json::to_string(&facts.cost).unwrap();
    assert!(
        !text.contains("0.0"),
        "an unpriced card is never zero: {text}"
    );
}

#[test]
fn atlas_stale_record_keeps_flag() {
    let export = derived(|v| {
        let i = record_index(v, "deepseek-v4-flash");
        v["records"][i]["stale"] = json!(true);
    });
    let facts = resolve(&export.card("deepseek-v4-flash").unwrap(), None);
    assert!(facts.stale, "the record is stale");
    assert!(facts.open_weight.stale());
    assert!(facts.context_window.stale());
    assert!(facts.reasoning_levels.stale());
    assert!(facts.lineage.maker.stale());
    assert!(facts.routes.iter().all(|r| r.stale));
    // ... and the resolved value itself is unchanged.
    assert_eq!(facts.open_weight.value(), Some(&true));
    // `stale: null` and an absent field both read as not stale.
    let null_stale = derived(|v| {
        let i = record_index(v, "deepseek-v4-flash");
        v["records"][i]["stale"] = Value::Null;
    });
    assert!(!resolve(&null_stale.card("deepseek-v4-flash").unwrap(), None).stale);
    assert!(!resolve(&pinned().card("deepseek-v4-flash").unwrap(), None).stale);
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
    for id in [LIST_ROUTE, FREE_ROUTE, FREE_ROUTE_WITH_CARD] {
        let (_, issues) = export.route_cost(id).unwrap();
        assert!(
            issues.iter().all(|i| i.code != "atlas_source_unresolved"),
            "{id}: {issues:?}"
        );
    }

    // A copy citing a source id the export does not carry is an issue, not a silent Known fact.
    let export = derived(|v| {
        let i = record_index(v, "qwen3.8-27b");
        v["records"][i]["open_weight"]["source"] = json!("src.absent");
    });
    let card = export.card("qwen3.8-27b").expect("the copy holds the card");
    let facts = resolve(&card, None);
    let issue = facts
        .issues
        .iter()
        .find(|i| i.code == "atlas_source_unresolved")
        .unwrap_or_else(|| panic!("no atlas_source_unresolved issue: {:?}", facts.issues));
    assert!(issue.message.contains("qwen3.8-27b"), "{}", issue.message);
    assert!(issue.message.contains("src.absent"), "{}", issue.message);
    assert!(facts.open_weight.value().is_some());
}
