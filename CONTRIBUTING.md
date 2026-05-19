# Contributing to subms-cookbook

Thanks for considering a contribution. The cookbook is a working notebook of
sub-millisecond engineering; PRs that keep the quality bar tight are welcome.

## The quality bar

Every recipe ships:

- A **reference implementation** it's validated against (e.g. JCTools for
  the SPSC ring buffer; `bumpalo` for the arena allocator). The recipe's
  results are compared against the reference in tests where the shape
  permits.
- A **sub-ms claim condition** statement: exactly which workload, dataset
  size, contention level, and hardware tier the p99 < 1 ms holds for.
- A **non-claims** statement: workloads / tiers explicitly NOT covered.
- **>= 90% line coverage** on the library code (Rust: tarpaulin; Java: jacoco).
  Harness adapters (`recipe.rs`, `PerfMain.java`, etc.) are excluded.
- **>= 10-12 tests per language**: every public method, edge cases, boundary
  conditions, negative tests, sequence tests, and property / stress tests.
- A **passing sub-ms assertion** in CI - if a future PR slows a hot path by
  more than the per-stage threshold, the gate catches it on the PR.

PRs that don't meet this bar will be asked for the missing pieces before
merge. See the README's "Conventions" section for the rest.

## The parity rule

Recipes ship **two** language surfaces - Rust and Java - that must stay
behavioural-equivalent at the public-API level. The JSON contract emitted
by the bench tests is byte-identical across them, and the API names map
1:1 modulo case style.

**Rule:** if you change one side, you change the other in the same PR.

PRs that only touch one language will be asked for the other side before
merge. If you genuinely can only implement one side, open the PR with a
clear note - someone will help port it.

## Quick rules

- Open an issue first for non-trivial changes so we can align on shape.
- One concern per PR; review remains tractable.
- ASCII-only in source + docs. No em-dash / en-dash / curly quotes.
- New behaviour comes with tests on both sides.
- New public API comes with rustdoc / javadoc + a README example.

## Local development

For a single recipe:

```bash
cd recipes/subms-bloom-filter
( cd rust && cargo test --all-features )
( cd java && mvn -q verify )
```

For all recipes (slow):

```bash
for d in recipes/*/rust;  do ( cd "$d" && cargo test --all-features ) || exit 1; done
for d in recipes/*/java;  do ( cd "$d" && mvn -q verify )              || exit 1; done
```

## Adding a new recipe

1. Open an issue with the proposed name, reference impl, and sub-ms claim
   conditions. Wait for design alignment before writing code.
2. Create `recipes/subms-<name>/{rust,java}/` following the template of an
   existing recipe (bloom-filter is the canonical example).
3. Implement library, tests (>=10-12, >=90% coverage), the `Recipe` adapter,
   and the `sub_millisecond_bench` assertion.
4. Add `content/recipes/subms-<name>/{index.md, rust.md, java.md, perf/}`.
5. Run the local pre-flight (per-language test + a clean
   `cargo doc --no-deps --all-features` + `mvn javadoc:javadoc`).
6. Open the PR. Bench numbers + reference-impl cross-checks go in the PR
   description.

## Release flow

Each artefact follows semver and releases on its own cadence:

1. Bump the artefact's `Cargo.toml` + `pom.xml` in lockstep (they're a
   matched Rust + Java pair).
2. Update the per-artefact CHANGELOG (where one exists).
3. Open a release PR; CI must be green.
4. Merge.
5. Tag: `git tag <artefact-name>-v<major>.<minor>.<patch>` and push the tag.
6. The release workflow publishes to crates.io + Maven Central.

## Reporting bugs

Open a GitHub issue with:

- The artefact + version
- A minimal reproduction (Rust or Java code that triggers the bug)
- What you expected vs what you got
- The full stack trace / panic output / JSON output if relevant

For security issues, see [SECURITY.md](SECURITY.md) - do not open a public
issue for vulnerabilities.

## License

By contributing, you agree your contributions are dual-licensed under the
[MIT License](LICENSE-MIT) and the [Apache License 2.0](LICENSE-APACHE),
matching the rest of the project.
