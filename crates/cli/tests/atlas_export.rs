//! S-12/T-25, S-13, seam-sync-1, S-16, S-17 and S-18: the pinned model-atlas export mapped to
//! tier-drop registry facts.
//!
//! Fixture: `crates/cli/tests/fixtures/model-atlas-export-2026.11.10-2.json`, vendored byte for
//! byte: `model-atlas.export/1`, export_version `2026.11.10-2`, sha256
//! `fe83a346aa4809dd1356747b0d55823f1a3166261703d128303930cf79741a9c`, produced by model-atlas
//! at `exports/model-atlas.export-2026.11.10-2.json`, and marked `current` in model-atlas
//! `exports/PINS.tsv`. The earlier fixtures are gone: S-13 replaced `2026.11.01-2`,
//! seam-sync-1 replaced `2026.11.01-4`, S-16 replaced `2026.11.02-3`, S-17 replaced
//! `2026.11.05-2` and S-18 replaced `2026.11.10-1`, each in one commit with `git rm`.
//!
//! Export 2026.11.10-2 carries 7 cards, 5 providers and 8 routes (one of them a free route with an
//! implied price, and one the OpenRouter route for `qwen3.8-27b`), so the route, cost-basis and
//! maker-id paths run on the pinned file. It has no unpriced route, so the unpriced path runs on a
//! DERIVED copy: the pinned bytes are parsed, one value is changed, and the copy is re-serialized in
//! memory. No file is written and the pinned fixture, with its sha256, is never touched.
//!
//! The producer marks records stale itself: all 5 providers and 7 of the 8 routes are stale, and no
//! card record is. Each card's expected staleness is read from the fixture (its own record's flag,
//! or the flag on any route that serves it) instead of being asserted false.
//!
//! Facts that moved with earlier pins are kept: `route.deepseek.deepseek-flash` belongs to the card
//! `deepseek-v4.1-flash`, so the card `deepseek-v4-flash` has no route at all; and the glm-5.3-flash
//! route states the same Coding Plan `unknown` on all three price members, so its reason states each
//! distinct reason once. The `observations.role_outcomes` tables are ignored by the reader: reading
//! them into resolved facts is out of scope for this slice.
//!
//! What moved with this pin: the export adds card `xiaomi-mimo-v2.6-pro` and its OpenRouter route
//! `route.openrouter.xiaomi-mimo-v2-6-pro`, so the card list grows to seven and the route list to
//! eight. The card states a context of 1,050,000 and names no reasoning levels; the route is marked
//! stale and prices 0.435 in / 0.87 out. The route's own `context_offered_tokens` is 1,050,000 and
//! its note keeps the snapshot's 1,048,576 top-provider figure beside it, unconverted: the reader
//! reports what the export states and a consumer admits on the smaller.

use std::path::Path;

use serde_json::{json, Value};
use tier_drop_cli::atlas::{resolve, Cost, CostBasis, Export};

const FIXTURE: &str = include_str!("fixtures/model-atlas-export-2026.11.10-2.json");
const PINNED_SHA256: &str = "fe83a346aa4809dd1356747b0d55823f1a3166261703d128303930cf79741a9c";
const PINNED_VERSION: &str = "2026.11.10-2";
const REPLACED_FIXTURE: &str = "model-atlas-export-2026.11.10-1.json";
const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

const CARD_IDS: [&str; 7] = [
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "deepseek-v4.1-flash",
    "glm-5.3-flash",
    "qwen3.8-27b",
    "xiaomi-mimo-v2.6-flash",
    "xiaomi-mimo-v2.6-pro",
];
const FREE_ROUTE: &str = "route.openrouter.nvidia-nemotron-3-5-super-vl-free";
const FREE_ROUTE_WITH_CARD: &str = "route.openrouter.nvidia-nemotron-3-5-super-vl";
const LIST_ROUTE: &str = "route.deepseek.deepseek-flash";
const LIST_ROUTE_CARD: &str = "deepseek-v4.1-flash";
const NO_ROUTE_CARD: &str = "deepseek-v4-flash";
const GLM_ROUTE: &str = "route.zai-coding-plan.glm-5-3-flash";
const MIMO_PRO_CARD: &str = "xiaomi-mimo-v2.6-pro";
const MIMO_PRO_ROUTE: &str = "route.openrouter.xiaomi-mimo-v2-6-pro";

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

/// The `stale` flag the export writes on the record with this id, read from the fixture.
fn record_stale(value: &Value, id: &str) -> bool {
    value["records"][record_index(value, id)]["stale"] == json!(true)
}

/// The route ids the export says serve this card.
fn route_ids_for_card(value: &Value, card_id: &str) -> Vec<String> {
    value["records"]
        .as_array()
        .expect("records is a list")
        .iter()
        .filter(|r| r["kind"] == json!("route") && r["card"] == json!(card_id))
        .filter_map(|r| r["id"].as_str().map(str::to_owned))
        .collect()
}

/// How many records of this kind the export marks stale.
fn stale_records_of_kind(value: &Value, kind: &str) -> usize {
    value["records"]
        .as_array()
        .expect("records is a list")
        .iter()
        .filter(|r| r["kind"] == json!(kind) && r["stale"] == json!(true))
        .count()
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
    assert_eq!(
        export.cards().len(),
        7,
        "export 2026.11.10-2 holds seven cards"
    );
    assert_eq!(
        export.routes().len(),
        8,
        "export 2026.11.10-2 carries eight routes"
    );

    // This is the first pinned export whose producer marks records stale itself. The inventory is
    // checked here so the staleness tests below cannot pass vacuously on an export that has no
    // stale record; the resolved expectation itself is derived in `atlas_every_card_resolves`.
    let value = pinned_value();
    assert_eq!(
        stale_records_of_kind(&value, "provider"),
        5,
        "all five providers are stale"
    );
    assert_eq!(
        stale_records_of_kind(&value, "route"),
        7,
        "seven of the eight routes are stale"
    );
    assert_eq!(
        stale_records_of_kind(&value, "card"),
        0,
        "no card record is stale"
    );

    // The re-pin replaced the previous fixture in one commit; the old bytes are not in the tree.
    let old = Path::new(MANIFEST_DIR)
        .join("tests")
        .join("fixtures")
        .join(REPLACED_FIXTURE);
    assert!(
        !old.exists(),
        "the 2026.11.10-1 fixture must be removed with git rm, not kept beside the pin"
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
    let value = pinned_value();
    // The list is the export's own card list: a card added to or dropped from either one is a
    // fact the pin has to answer for.
    assert_eq!(
        CARD_IDS.len(),
        export.cards().len(),
        "the card list and the pinned export disagree"
    );
    for id in CARD_IDS {
        let card = export.card(id).unwrap_or_else(|| panic!("no card `{id}`"));
        let facts = resolve(&card, None);
        assert!(
            facts.issues.iter().all(|i| i.code != "atlas_fact_missing"),
            "{id}: {:?}",
            facts.issues
        );
        assert_eq!(facts.card_id, id);
        // Staleness is the export's own: the card's record flag, or the flag on any route that
        // serves it. Read from the fixture, never asserted false.
        let own = record_stale(&value, id);
        let routes = route_ids_for_card(&value, id);
        let any_route_stale = routes.iter().any(|route| record_stale(&value, route));
        assert_eq!(
            facts.stale,
            own || any_route_stale,
            "{id}: resolved staleness must follow the export's own flags"
        );
        if routes.is_empty() {
            assert_eq!(
                facts.stale, own,
                "{id}: a card read with no route keeps its own record's flag"
            );
        }
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
    let card = export.card(LIST_ROUTE_CARD).unwrap();
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

/// S-16: the pin moved `route.deepseek.deepseek-flash` from `deepseek-v4-flash` to the new card
/// `deepseek-v4.1-flash`, and the old card is left with no route at all.
#[test]
fn atlas_flash_route_belongs_to_v41() {
    let export = pinned();

    let route = export
        .route(LIST_ROUTE)
        .unwrap_or_else(|| panic!("no route `{LIST_ROUTE}`"));
    assert_eq!(route.card.as_deref(), Some(LIST_ROUTE_CARD));
    assert_eq!(route.card_unknown, None);

    // Resolved by route id, the route names its card and carries a listed price.
    let (cost, issues) = export.route_cost(LIST_ROUTE).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cost.card_id.as_deref(), Some(LIST_ROUTE_CARD));
    assert_eq!(cost.cost_basis, CostBasis::List);
    assert!(cost.cost.is_known());

    // The card it belongs to resolves the same single route, so its cost is that route's price.
    let v41 = resolve(&export.card(LIST_ROUTE_CARD).unwrap(), None);
    assert_eq!(v41.routes.len(), 1, "{:?}", v41.routes);
    assert_eq!(v41.routes[0].route_id, LIST_ROUTE);
    assert_eq!(v41.cost.value(), cost.cost.value());

    // The old card has no route, so its cost is Unknown with "no route in export" -- never 0.
    let old = export
        .card(NO_ROUTE_CARD)
        .unwrap_or_else(|| panic!("no card `{NO_ROUTE_CARD}`"));
    let facts = resolve(&old, None);
    assert!(facts.routes.is_empty(), "{:?}", facts.routes);
    assert_eq!(facts.cost.value(), None);
    assert_eq!(facts.cost.reason(), Some("no route in export"));
    let text = serde_json::to_string(&facts.cost).unwrap();
    assert!(
        !text.contains("0.0"),
        "a card with no route is never zero: {text}"
    );
}

/// S-16: the glm-5.3-flash route states the same Coding Plan `unknown` on all three price members.
/// Joined plainly, the reason repeats the clause; it states each distinct reason once and the
/// cost stays Unknown (a subscription's zero marginal price is not a measured bill).
#[test]
fn atlas_unknown_cost_reason_has_no_repeated_clause() {
    const CLAUSE: &str = "Coding Plan subscription";
    let export = pinned();

    let (cost, issues) = export
        .route_cost(GLM_ROUTE)
        .unwrap_or_else(|| panic!("no route `{GLM_ROUTE}`"));
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cost.cost_basis, CostBasis::Actual);

    // Unknown, and never Known(0): the reason is the fact, not an invented zero.
    assert_eq!(cost.cost.value(), None, "the plan's tariff is Unknown");
    assert_ne!(
        cost.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.0,
            output_usd_per_mtok: 0.0,
        }),
        "an unknown tariff is never taken as a real zero"
    );
    let reason = cost
        .cost
        .reason()
        .expect("the cost is Unknown with a reason");
    assert!(!reason.is_empty());

    // Each distinct reason appears once. A plain join of the input and output reasons repeats it.
    assert_eq!(
        reason.matches(CLAUSE).count(),
        1,
        "the reason repeats the clause: {reason}"
    );
    // The fuller of the two reasons survives: the output reason is contained in it, so dropping the
    // contained copy loses nothing.
    assert!(
        reason.contains("not a measured bill"),
        "the fuller reason was dropped: {reason}"
    );

    // The card resolves the same single route, so its cost is the same Unknown with one clause.
    let card = export.card("glm-5.3-flash").unwrap();
    let facts = resolve(&card, None);
    assert_eq!(facts.routes.len(), 1, "{:?}", facts.routes);
    assert_eq!(facts.cost.value(), None);
    let card_reason = facts
        .cost
        .reason()
        .expect("the card's cost states a reason");
    assert_eq!(card_reason.matches(CLAUSE).count(), 1, "{card_reason}");
}

/// S-18: the export that adds card `xiaomi-mimo-v2.6-pro` and its OpenRouter route. The card is
/// read through that route, and every fact it carries is the export's own: a listed price with a
/// source the export holds, a known maker, no reasoning levels (with the export's reason), the
/// route's stated context, and the route's stale flag on the result.
#[test]
fn atlas_mimo_pro_route_resolves() {
    let export = pinned();
    let value = pinned_value();

    let card = export
        .card(MIMO_PRO_CARD)
        .unwrap_or_else(|| panic!("no card `{MIMO_PRO_CARD}`"));
    let route = export
        .route(MIMO_PRO_ROUTE)
        .unwrap_or_else(|| panic!("no route `{MIMO_PRO_ROUTE}`"));
    assert_eq!(route.card.as_deref(), Some(MIMO_PRO_CARD));

    let facts = resolve(&card, Some(&route));
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);
    assert_eq!(facts.card_id, MIMO_PRO_CARD);
    assert_eq!(facts.route_id.as_deref(), Some(MIMO_PRO_ROUTE));

    // The price is Known, with the source the export holds for it.
    assert_eq!(
        facts.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.435,
            output_usd_per_mtok: 0.87,
        })
    );
    let source = facts.cost.source().expect("a Known cost names its source");
    assert_eq!(source, "src.models-dev-api");
    assert!(
        export.known_sources().contains(source),
        "{source} must resolve in the export"
    );

    // The maker is Known; no reasoning levels are stated, and the reason is the export's own.
    assert_eq!(
        facts.lineage.maker.value().map(String::as_str),
        Some("xiaomi")
    );
    assert!(!facts.lineage.maker_free_text);
    assert_eq!(facts.reasoning_levels.value(), None);
    let reason = facts
        .reasoning_levels
        .reason()
        .expect("an Unknown fact states the export's reason");
    assert!(reason.contains("name no levels"), "{reason}");
    assert_eq!(facts.lineage.key(), None, "the architecture is not stated");

    // The context is the value the route itself states, not the card's and not a conversion.
    let route_stated = value["records"][record_index(&value, MIMO_PRO_ROUTE)]
        ["context_offered_tokens"]["value"]
        .as_u64()
        .expect("the route states a context");
    assert_eq!(facts.context_window.value(), Some(&route_stated));
    assert_eq!(facts.context_window.value(), Some(&1_050_000));
    assert_eq!(facts.context_window.source(), Some("src.openrouter-models"));

    // The pin marks this route stale, and the result follows the record's own flag.
    assert_eq!(
        facts.stale,
        record_stale(&value, MIMO_PRO_ROUTE),
        "the result's flag is the route record's flag"
    );
    assert!(facts.stale);
    assert!(facts.context_window.stale());
    assert!(facts.cost.stale());
}

/// S-17: this is the first pinned export whose producer marks records stale itself. A read
/// through a stale route carries the flag on the result and on the facts the route supplies, and
/// a stale fact is neither dropped nor turned Unknown because it is stale.
#[test]
fn atlas_stale_route_carries_flag() {
    let export = pinned();
    let value = pinned_value();

    // The flag is the export's own, read from the fixture before anything is resolved.
    assert!(
        record_stale(&value, LIST_ROUTE),
        "the pin marks {LIST_ROUTE} stale"
    );
    let route = export
        .route(LIST_ROUTE)
        .unwrap_or_else(|| panic!("no route `{LIST_ROUTE}`"));
    assert!(route.is_stale(), "the parsed route is stale");

    // Read through the stale route: the result is stale, and so are the facts the route supplies.
    let card = export.card(LIST_ROUTE_CARD).unwrap();
    let facts = resolve(&card, Some(&route));
    assert!(facts.stale, "a read through a stale route is stale");
    assert_eq!(facts.route_id.as_deref(), Some(LIST_ROUTE));
    assert_eq!(facts.routes.len(), 1, "{:?}", facts.routes);
    assert!(facts.routes[0].stale, "the route cost is stale");

    // The route-derived context window keeps its value and its provenance, marked stale.
    assert!(facts.context_window.stale());
    assert_eq!(facts.context_window.value(), Some(&1_048_576));
    assert_eq!(facts.context_window.source(), Some("src.models-dev-api"));

    // The cost stays Known with its value and source, marked stale -- not dropped, not Unknown.
    assert!(facts.cost.stale());
    assert_eq!(
        facts.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.15,
            output_usd_per_mtok: 0.6,
        })
    );
    assert_eq!(facts.cost.source(), Some("src.deepseek-pricing"));
    assert!(facts.issues.is_empty(), "{:?}", facts.issues);

    // The same read by route id, without a card, carries the flag on the cost fact too.
    let (cost, issues) = export.route_cost(LIST_ROUTE).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert!(cost.stale, "the route cost is stale");
    assert!(cost.cost.stale());
    assert!(cost.cost.is_known());
    assert_eq!(cost.cost_basis, CostBasis::List);
}

/// S-17: the export carries `observations.role_outcomes`, and the reader ignores them without
/// error. Reading them into resolved facts is out of scope for this slice.
#[test]
fn atlas_export_with_role_outcomes_parses() {
    let value = pinned_value();

    // The raw JSON carries at least one role-outcomes entry, so this test is not vacuous.
    let records = value["records"].as_array().expect("records is a list");
    let with_role_outcomes = records
        .iter()
        .filter(|r| {
            r["observations"]["role_outcomes"]
                .as_array()
                .is_some_and(|entries| !entries.is_empty())
        })
        .count();
    assert!(
        with_role_outcomes >= 1,
        "the pin carries at least one observations.role_outcomes entry"
    );

    // The reader parses the export, and sees the same cards and routes the raw JSON states.
    let export = Export::parse(FIXTURE).expect("the export with role outcomes parses");
    let cards = records
        .iter()
        .filter(|r| r["kind"] == json!("card"))
        .count();
    let routes = records
        .iter()
        .filter(|r| r["kind"] == json!("route"))
        .count();
    assert_eq!(export.cards().len(), cards);
    assert_eq!(export.routes().len(), routes);

    // A route that carries role outcomes still resolves its facts as the export states them.
    let route = export
        .route(LIST_ROUTE)
        .expect("the route with outcomes parses");
    let (cost, issues) = export.route_cost(&route.id).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(
        cost.cost.value(),
        Some(&Cost {
            input_usd_per_mtok: 0.15,
            output_usd_per_mtok: 0.6,
        })
    );
}

#[test]
fn atlas_lineage_from_maker_id() {
    let export = pinned();
    let expected = [
        ("deepseek-v4-flash", "deepseek"),
        ("deepseek-v4-pro", "deepseek"),
        ("deepseek-v4.1-flash", "deepseek"),
        ("glm-5.3-flash", "zhipuai"),
        ("qwen3.8-27b", "qwen"),
        ("xiaomi-mimo-v2.6-flash", "xiaomi"),
        ("xiaomi-mimo-v2.6-pro", "xiaomi"),
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
        "deepseek-v4.1-flash",
        "glm-5.3-flash",
        "xiaomi-mimo-v2.6-flash",
        "xiaomi-mimo-v2.6-pro",
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
    // The pinned cards all have a route except `deepseek-v4-flash`; a copy with a route removed
    // states no price. `deepseek-v4.1-flash` is the card that carries `LIST_ROUTE` in the pin, so
    // removing it here exercises the derived path rather than a fact the pin already holds.
    let export = derived(|v| {
        let i = record_index(v, LIST_ROUTE);
        v["records"][i]["card"] = Value::Null;
        v["records"][i]["card_unknown"] = json!("no card for this route in the derived copy");
    });
    let facts = resolve(&export.card(LIST_ROUTE_CARD).unwrap(), None);
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
    for id in [LIST_ROUTE, FREE_ROUTE, FREE_ROUTE_WITH_CARD, GLM_ROUTE] {
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
