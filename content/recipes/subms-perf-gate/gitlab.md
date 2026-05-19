## GitLab CI - coming soon

The actions are pure Node std-lib (no `npm install`, no Docker). The same JSON contract works for GitLab CI; what's not yet shipped is a polished `.gitlab-ci.yml` template + a GitLab-native MR comment helper to match the sticky-PR-comment behaviour the GitHub Action provides.

The intended shape (planned, not yet released):

```yaml
# .gitlab-ci.yml (sketch - the actual template will be refined)
stages: [perf]

subms-perf-diff:
  stage: perf
  image: node:20
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
  script:
    # Snapshot the target branch's perf JSON
    - git fetch origin $CI_MERGE_REQUEST_TARGET_BRANCH_NAME
    - git show origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME:perf/myworkload.json > baseline.json
    # Use the merge request's perf JSON
    - cp perf/myworkload.json candidate.json
    # Run the same diff math the GitHub Action does
    - npx -y subms-perf-diff --baseline baseline.json --candidate candidate.json --threshold 15
  artifacts:
    when: always
    reports:
      junit: subms-diff.junit.xml         # planned - JUnit-flavoured output for GitLab's test widget
    paths:
      - subms-diff.json
      - subms-diff.md
```

What's blocking the release:

- **MR-comment helper.** GitHub's `actions/github-script` doesn't exist on GitLab. We need a small Node script that calls GitLab's [Notes API](https://docs.gitlab.com/ee/api/notes.html) to post / update a sticky MR note. ~100 LOC.
- **JUnit-style report emission.** GitLab's MR widget surfaces JUnit XML inline; we'd add an `--emit-junit` flag to `subms-diff` that maps regressing stages to JUnit failures.
- **`npx -y subms-perf-diff` CLI package.** The Node script that powers the GitHub Action needs to be wrapped as an npm package so GitLab pipelines can `npx` it. ~30 minutes of work.

If you need GitLab support, please open an issue at [`submillisecond/subms-actions`](https://github.com/submillisecond/subms-actions/issues/new). The work is small but unscheduled.

In the meantime, you can use the actions on a GitHub mirror of your GitLab repo, or run the Node scripts directly inside a GitLab job (the diff math is in [`diff.js`](https://github.com/submillisecond/subms-action-diff/blob/main/diff.js) - copy that file in and invoke `node diff.js` with the right env vars).
