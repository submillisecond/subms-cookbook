# Changelog

All notable changes to `subms-cookbook` are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: per-artefact semver - each recipe / primer carries its own
`Cargo.toml` / `pom.xml` version. This top-level changelog documents
repo-wide events (extractions, conventions, infra changes), not individual
artefact bumps. Per-artefact release notes live in GitHub Releases.

## [Unreleased]

### Changed
- IA rename: `guides/` -> `primers/` (and the Java package + Maven groupId
  rename from `com.submillisecond.guides.*` to `com.submillisecond.primers.*`).
  CI workflow + summary script updated. None of the three primer artefacts
  have been published yet, so this is a free rename.
- IA rename: `topics/` deprecated. The theme-cluster pages
  (probabilistic-data-structures, ordered-indexes, etc.) are removed; theme
  is now a faceted filter on the cookbook index derived from each recipe's
  `category` field. New `stacks/<slug>/` content type takes the
  application-domain-blueprint slot (DeFi, HFT, OLTP), with embedded
  `<component>/` subdirectories.
- Flat layout: the `content/` dir is gone. Each recipe and primer now keeps
  code and writeup in one directory (e.g. `recipes/subms-bloom-filter/`
  contains `rust/`, `java/`, `index.md`, `rust.md`, `java.md`, `perf/`).
  Stacks live at the top level under `stacks/`. The subms-ui fetch script
  pulls the three top-level dirs (`recipes/`, `primers/`, `stacks/`) and
  filters out build-artefact dirs (`target/`, `node_modules/`).

## [0.1.0] - 2026-05-19

Extracted from the [submillisecond.com monorepo](https://github.com/submillisecond/submillisecond.com)
into a standalone repository: [`github.com/submillisecond/subms-cookbook`](https://github.com/submillisecond/subms-cookbook).

This is a repo-extraction release. The recipe and guide artefacts keep their
existing crates.io / Maven Central versions (no behavioural change). Future
bumps will follow per-artefact semver as before.

### Added
- Standalone repository with full governance: README, MIT + Apache-2.0 dual
  licence, SECURITY.md, CODE_OF_CONDUCT.md, CONTRIBUTING.md, .gitignore.
- Repo-wide `ci.yml` workflow: cargo + mvn test matrix per recipe; mvn test
  on guides; npm test on the cli.

### Changed
- Recipe Rust `Cargo.toml` files dropped the `path = "../../../subms/rust"`
  portion of their optional `subms` dep. The dep now resolves from crates.io
  as `subms = { version = "0.2.2-rc1", optional = true }` once the
  `harness` feature is enabled. Cross-recipe path deps
  (`subms-lsm-tree` -> `subms-bloom-filter`) stay intact since both still
  live as siblings under `recipes/`.
- Layout flattened from `cookbook/recipes/<name>/` to `recipes/<name>/` and
  `cookbook/guides/<lang>/<name>/` to `guides/<lang>/<name>/`. CLI moved
  from `cli/subms/` to `cli/`. Writeups + perf JSON from
  `apps/web/content/cookbook/` moved to `content/`.

### Removed
- The harness library (`cookbook/subms/`) - now lives in its own repo at
  [`submillisecond/subms`](https://github.com/submillisecond/subms).
- Monorepo-shaped publishing instructions referring to `.scripts/publish.ps1`
  and `cookbook/subms/java`. Per-artefact `release.yml` workflows replace
  the central publish script.
