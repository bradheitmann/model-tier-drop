# Owner decision 2026-11-04: per-repository Doppler project and semantic attestation key

## Decision
The owner directed, on 2026-11-04, that each workspace repository has its own Doppler project and its own signing key for Mister Clean semantic attestation, with Doppler as the canonical home. Owner's words: "I want to make sure that each repo here in this workspace has its own separate Doppler project. … make sure they have their own separate key and attestation stored in Doppler". The convention and its reasons are recorded in the mister-clean repository, `charter/owner-decisions/2026-11-04-semantic-attestation-trust.md` (commit 68fa0c3, mister-clean PR #71; open, not yet on main when this record was written).

## What exists for this repository
- Doppler project `model-tier-drop`; `doppler.yaml` at the root links it with default config `dev` for ordinary secrets.
- Config `attest`, in its own Doppler environment, so it inherits nothing from `dev`. It holds only `MISTER_CLEAN_ATTESTATION_PRIVATE_KEY_PEM` (masked), `MISTER_CLEAN_ATTESTATION_PUBLIC_KEY_PEM` and `MISTER_CLEAN_ATTESTATION_KEY_ID`.
- Key id `model-tier-drop-attest-ed25519-20261104`, Ed25519; SPKI DER sha256 begins `a5f9117d890e4847` (checked by PM-MODEL-TIER-DROP from the stored public key on 2026-11-04).
- Role `independent_qa` only, for the reviewer seat `MTD/QA:pi:xiaomi/mimo-v2.6-flash:openrouter` (a different family from the authors, Claude Opus 5.5 and deepseek-flash). Validity to 2027-02-02.
- Trust policy `model-tier-drop-semantic-trust-2026-11-04` (`mister-clean.semantic-trust-policy` schema 1.0), ACTIVE: stored in `attest` as `MISTER_CLEAN_TRUST_POLICY_JSON` (with `MISTER_CLEAN_TRUST_POLICY_SHA256`); sha256 `7fb3ddfbd88c3bc95e5ea1ed315cab76cc4e81361267a11100b851f42944c101` over the exact stored bytes (no trailing newline; materialize with `printf "%s"` inside `doppler run`). Checked by PM-MODEL-TIER-DROP on 2026-11-04: the hash matches, the policy names only the actor, key, role and validity above, and its public key fingerprint is `a5f9117d890e4847`. Owner approval 2026-11-04 ("Approve all."; mister-clean PR #71 at 24a202c). PM-MODEL-TIER-DROP re-hashed the active bytes after promotion: the sha is unchanged and the draft variables are gone.

## Rules
- The key reaches a signer only through `doppler run --project model-tier-drop --config attest -- <command>`. It is never written to a file, committed, quoted or sent.
- Only the reviewer seat signs. The author and the PM never sign their own work.

## Limits
Every seat runs under the owner's one OS user and Doppler login, so any process can fetch the key. Custody is by rule plus Doppler's access log, not enforced; this is the same boundary as charter TODO 94 (receipt identity). An enforced boundary (per-seat OS users, or a per-use prompt) is an owner decision.

The owner accepted this limit on 2026-11-04 as custody option (a): independence rests on rule and the Doppler access log, not on separate OS users (mister-clean PR #71 at 24a202c).

## Amendment 2026-11-04: inspection only through the helper
After another repository's reviewer seat printed its attestation private key (a sed that stripped only the first line of a multi-line value), PM-MISTER-CLEAN set a fleet rule: no agent runs its own command against an `attest` config. Inspection uses only the read-only helper `attest-inspect model-tier-drop`, which prints key id, public fingerprint, private-key presence and length, the policy sha against the recorded one, and any revocation, never a value. Every brief that touches attestation carries this rule (AGENTS.md, "Landing a slice here", step 4).

Exposure check for this repository, by PM-MODEL-TIER-DROP on 2026-11-04 (counts and names only, no values printed): the only sessions that ran commands against `model-tier-drop/attest` were this PM's and PM-MISTER-CLEAN's setup session; neither has a private-key marker in any tool result; no MTD/DEV or MTD/QA session (Pi, Codex, Claude) ran one. The PM's own commands printed names, lengths, the key id, public fingerprints and the policy sha only. `attest-inspect` then reported: private key present (length 118) and matching the public key, policy active sha equal to the recorded `7fb3ddfb…c101`, no draft, not revoked. No rotation needed.
