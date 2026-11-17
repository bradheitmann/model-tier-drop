# Amendment 2026-11-17: pin the Worker build tool

Status: written by PM-MODEL-TIER-DROP on 2026-11-17. Binds slice S-19 / V-19 and amends S-06 (the build command) and S-11 (what is deployed).

## Need

CI on main has been red on every push since 2026-11-12T00:13Z (commit a3ee1ab). The step `worker build` runs `cargo install worker-build --locked`, which installs the newest release. worker-build 0.9.0 was published on 2026-11-11T23:44Z and refuses this Worker: "Unsupported version worker@0.8.7, expected at least worker@0.9.0" (CI run 43980161830, job `rust`, step 10).

The deploy has the same fault. `crates/worker/wrangler.jsonc` sets `build.command` to `cargo install -q worker-build --locked && worker-build --release`, so `wrangler deploy` would replace the host's worker-build 0.8.7 with 0.9.0 and then fail before any upload. S-11, which the owner released on 2026-11-16, cannot run until the tool is pinned.

## Rules

- Every command in this repository that installs worker-build names an exact version: `worker-build@0.8.7`. That was the newest release (published 2026-09-25) when the deployment of 2026-10-19 was built, it is the version installed on the deploy host today, and its `worker` crate (0.8.7) is the one in `crates/worker/Cargo.lock`.
- The pin and the locked `worker` crate share major.minor. A test in the Worker crate holds them together, so moving one without the other is red.
- No dependency changes: `crates/worker/Cargo.toml` and `crates/worker/Cargo.lock` are untouched.

## Order

S-19 lands on main first, which turns CI green again. Then main is merged into s14, so the deploy tip carries the pinned build tool and the tokio 1.54.1 lock from slice worker-tokio. S-11 deploys that tip. The merge changes nothing under `crates/worker/src`: since the two branches parted, main changed only `crates/worker/Cargo.lock` and `crates/worker/README.md` in that crate.

## Not in this amendment

Moving the Worker to `worker` 0.9. It is a dependency change with its own tests and a redeploy, recorded as an obligation in charter/STATE.md.
