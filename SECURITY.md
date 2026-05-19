# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in any recipe, guide, or the CLI in
this repo, please **do not** open a public issue. Use GitHub's private
vulnerability reporting:

> Repository -> Security -> Report a vulnerability

Or email the maintainers privately at `security@submillisecond.com`. Provide:

- a description of the issue
- steps to reproduce
- the artefact name (e.g. `subms-bloom-filter`), language, and version you
  observed it on
- any proof-of-concept code or output

We aim to acknowledge reports within **5 business days** and to publish a fix
or mitigation within **30 days** of acknowledgement, depending on severity.

## Supported versions

The latest tagged release of each artefact receives security fixes. Older
tags are best-effort. Once an artefact ships v1.0.0, the previous major
will receive security fixes for 6 months.

## Scope

In scope:

- The recipe libraries under `recipes/<name>/` (Rust + Java).
- The guide examples under `guides/`.
- The CLI under `cli/`.
- The `content/` markdown if it contains code samples that would mislead a
  reader into an insecure pattern.

Out of scope:

- The `subms` perf harness itself - report at
  [`submillisecond/subms`](https://github.com/submillisecond/subms).
- The Next.js renderer - report at
  [`submillisecond/subms-ui`](https://github.com/submillisecond/subms-ui).
- The CI gate actions - report at
  [`submillisecond/subms-actions`](https://github.com/submillisecond/subms-actions).
- Findings against third-party libraries listed in a recipe's docs purely
  for cross-comparison (`JCTools`, `Caffeine`, `bumpalo`, etc.) - report to
  their upstream maintainers.
- Sub-millisecond performance numbers being lower than published on a given
  hardware tier. The library reports what it measures; it does not warrant
  absolute performance.
