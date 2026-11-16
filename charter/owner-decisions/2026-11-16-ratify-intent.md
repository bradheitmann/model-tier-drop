# Owner decision 2026-11-16: intent card ratified; frontier stays on the table; S-11 deploy approved

## Decisions
1. **INTENT.md ratified.** The owner reviewed the card, with three reworded lines (outcome, largest uncertainty, frontier), and said on 2026-11-16: "Intent is ratified."
2. **Frontier is not banned by default.** The owner, 2026-11-16: "I don't have a Frontier ban for most things, but I may have some for specific things." The 2026-10-31 ruling stands: if the user provided the inference, frontier is on the table. A ban is per consumer and per decision. The only one on record is lumbr's (lumbr charter/owner-decisions/2026-11-11-ai-native-logging-platform.md: "this work should probably never ascend to a frontier model level"). A consumer applies its ban in its own registry and policy, never in this engine.
3. **S-11 approved.** The owner, 2026-11-16: "I do approve deploying the worker, and I approve the escalation work merged in." This releases the repository for S-11 (the Worker deploy) and the merge of s10 and s14 to main. The Cloudflare token is the owner's to type into Doppler `model-tier-drop/dev` as `CLOUDFLARE_API_TOKEN`; agents check it by shape only, never print it.

## Scope
The rest of the park of 2026-11-06 stands. The release covers S-11 and the s10/s14 merge, plus the record-only changes it needs (STATE, INTENT, receipts).
