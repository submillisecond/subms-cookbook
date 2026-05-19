# Changelog

All notable changes to `subms-cookbook` are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: per-artefact semver - each recipe / guide carries its own
`Cargo.toml` / `pom.xml` version. This top-level changelog documents
repo-wide events (extractions, conventions, infra changes), not individual
artefact bumps. Per-artefact release notes live in GitHub Releases.

## [Unreleased]

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
