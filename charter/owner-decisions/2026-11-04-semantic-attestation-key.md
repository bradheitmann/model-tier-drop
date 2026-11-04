# Owner decision 2026-11-04: per-repository Doppler project and semantic attestation key

## Decision
The owner directed, on 2026-11-04, that each workspace repository has its own Doppler project and its own signing key for Mister Clean semantic attestation, with Doppler as the canonical home. Owner's words: "I want to make sure that each repo here in this workspace has its own separate Doppler project. … make sure they have their own separate key and attestation stored in Doppler". The convention and its reasons are recorded in the mister-clean repository, `charter/owner-decisions/2026-11-04-semantic-attestation-trust.md` (commit 68fa0c3, mister-clean PR #71; open, not yet on main when this record was written).

## What exists for this repository
- Doppler project `model-tier-drop`; `doppler.yaml` at the root links it with default config `dev` for ordinary secrets.
- Config `attest`, in its own Doppler environment, so it inherits nothing from `dev`. It holds only `MISTER_CLEAN_ATTESTATION_PRIVATE_KEY_PEM` (masked), `MISTER_CLEAN_ATTESTATION_PUBLIC_KEY_PEM` and `MISTER_CLEAN_ATTESTATION_KEY_ID`.
- Key id `model-tier-drop-attest-ed25519-20261104`, Ed25519; SPKI DER sha256 begins `a5f9117d890e4847` (checked by PM-MODEL-TIER-DROP from the stored public key on 2026-11-04).
- Role `independent_qa` only, for the reviewer seat `MTD/QA:pi:xiaomi/mimo-v2.6-flash:openrouter` (a different family from the authors, Claude Opus 5.5 and deepseek-flash). Validity to 2027-02-02.
- Trust policy sha256: pending (PM-MISTER-CLEAN drafts it; it is added here when stored in `attest`).

## Rules
- The key reaches a signer only through `doppler run --project model-tier-drop --config attest -- <command>`. It is never written to a file, committed, quoted or sent.
- Only the reviewer seat signs. The author and the PM never sign their own work.

## Limits
Every seat runs under the owner's one OS user and Doppler login, so any process can fetch the key. Custody is by rule plus Doppler's access log, not enforced; this is the same boundary as charter TODO 94 (receipt identity). An enforced boundary (per-seat OS users, or a per-use prompt) is an owner decision.
