# Agent instructions for model-tier-drop

- Rust by default. Strict TypeScript only in `adapters/dsh` (dsh plugins are Node modules). No Python. No authored `.js/.mjs/.cjs` files; `crates/worker/build/` and `adapters/dsh/dist/` are generated and ignored.
- The engine (`crates/core`) never calls a model and never touches the filesystem, the clock or the network. Harnesses execute; the engine plans and records.
- Model names are configuration (`registry/registry.example.json`), never code.
- Secrets never enter the repository. Run `gitleaks protect --staged --redact` before every commit and `gitleaks detect --redact` before every push. Check that a key exists by its length, never by printing it.
- `main` receives only slices accepted by a separate QA context against `plan/contracts.md`.
- Plain, specific prose. Never write "needs you".
- The plan lives in `plan/`; change the contracts there before changing the code they bind.
