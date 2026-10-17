# Security reporting and contribution gates

Use this repository's private vulnerability reporting channel for sensitive
reports. Do not attach personal photos, libraries, model weights, credentials or
unredacted logs to public issues.

Main requires a pull request and completed checks for its current head. CI has no
release keys, deployment credentials or publishing permission. Public-tree checks
reject private engine paths and local data. CodeQL scans the current distribution
repository's Actions, Python gate code and Swift verifier on PR creation and every
push. Failed, cancelled or missing analysis blocks merging. These are automated
security checks, not a claim that a human or Codex review has completed.

The unpublished client and proprietary core implementation have separate review
and release gates. JavaScript/Rust analysis and full client builds/tests activate automatically
when the held client source is added; their absent-source steps make no client
qualification claim. A
passing documentation-only probe does not qualify unseen client or engine code.
Production releases additionally require completed exact-candidate review,
independently authenticated artifacts and migration/update/rollback evidence.

Workflow and policy changes need explicit maintainer review. Fork/Dependabot jobs
use the same read-only checkout path; no pull_request_target job executes PR code.
No workflow publishes a release, changes the update feed or deploys a website.
