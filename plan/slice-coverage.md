# Slice coverage (full horizon)

Denominator: 38 requirements in `plan/requirements.md` (R-01…R-07, R-10…R-24, R-30…R-35, R-40…R-43, R-50…R-55). Every one has an implementation slice and a paired verifier. Nothing is deferred inside the horizon; the explicitly deferred items are listed at the end.

| Requirement | Implemented by | Verified by | Tests |
|---|---|---|---|
| R-01 … R-06 | S-04 | V-04 | T-01 … T-06 |
| R-10 … R-15, R-17 … R-19, R-22 | S-02 | V-02 | T-10 … T-15, T-17 … T-19, T-22 |
| R-16, R-20, R-21 | S-03 | V-03 | T-16, T-20, T-21 |
| R-30 … R-34 | S-05 | V-05 | T-30 … T-34 |
| R-35 | S-06 | V-06 | T-35 |
| R-40 … R-43 | S-07 | V-07 | T-40 … T-43 |
| R-07, R-23, R-24 (amendment 2026-10-31); R-20, R-21, R-32 amended | S-10 (deploy: S-11) | V-10, V-11 | T-07, T-20, T-21, T-23, T-24, T-32 |
| R-50, R-51, R-53 (licence), R-55 | S-01 and S-09 | V-01 and V-09 | T-50, T-51, T-53, T-55 |
| R-52, R-53 (publication) | S-09 | V-09 | T-52, T-53 |
| R-54 | S-01 (prose) and S-08 (graph, compiled build) | V-01 and V-08 | T-54 |

## Cross-cutting categories

| Category | Covered by |
|---|---|
| Security | R-34 (Worker request hygiene), R-51 (secret hygiene), R-40 (paths never from tool arguments) |
| Data integrity and lifecycle | R-16 (append-only ledger with a per-record hash), R-19 (rights on every attempt) |
| Telemetry / observability | Worker `observability.enabled` (R-35 deployment), ledger as the engine's own wide events |
| Release / delivery | R-50 CI, R-53 publication, R-35 deploy with rollback command documented |
| Recovery / operations | `wrangler rollback` (S-06), ledger verify (R-16) |
| Compliance with owner rules | R-22 strict review, R-55 language/wording rules, R-06 model names as configuration |
| Accessibility / design | Not applicable (no UI); recorded in requirements |

## Counts

Implementation slices: 9 (S-01 … S-09). Verification slices: 9 (V-01 … V-09). Total: 18. Requirements selected: 35. Deferred: 0 inside the horizon.

## Explicitly out of scope (recorded, not silently dropped)

- A built Pi adapter (described in `adapters/README.md`, R-43).
- A shared remote ledger (D1) behind the Worker.
- Model execution inside the engine.
- Automatic QoS measurement; QoS values are measured by the operator or harness and written into the registry.
- The lw-engine-reveng amendment and the Intake roadmap placement (other repositories).
