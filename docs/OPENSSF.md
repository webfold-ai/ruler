# OpenSSF: Scorecard and Best Practices

Status of each control, as of this commit. "Met" means the repository contains what is needed.
"Needs a person" means the check depends on history, on other people, or on an action outside the
repository.

## Scorecard checks

| Check | Status | Where / why |
|---|---|---|
| Binary-Artifacts | Met | No binaries are committed. |
| Branch-Protection | Needs a person | Set the default-branch ruleset in GitHub settings. See the ruleset note below. |
| CI-Tests | Met | `ci.yml` runs on every PR. |
| CII-Best-Practices | Needs a person | Register the project at bestpractices.dev, then add the badge. |
| Code-Review | Needs a person | Depends on approved PRs in the history. |
| Contributors | Needs a person | Depends on contributors from more than one organisation. |
| Dangerous-Workflow | Met | No `pull_request_target`; no untrusted input in `run:` steps. |
| Dependency-Update-Tool | Met | `dependabot.yml`. |
| Fuzzing | Met | `fuzz/` with `libfuzzer-sys` targets; `fuzz.yml` runs them. |
| License | Met | `LICENSE-MIT`, `LICENSE-APACHE`. |
| Maintained | Needs time | Depends on repository age and activity. |
| Packaging | Not applicable | Not published to a package registry (`publish = false`). The release is a signed archive. |
| Pinned-Dependencies | Met | Every action is pinned by SHA. `cargo` uses `--locked`. |
| SAST | Met | `codeql.yml` (Rust) and `scorecard.yml`. |
| Security-Policy | Met | `SECURITY.md`, with a response target marked TODO. |
| Signed-Releases | Met | `release.yml` produces cosign signatures and SLSA provenance. |
| Token-Permissions | Met | Top-level `permissions:` in every workflow. |
| Vulnerabilities | Met, verify | `cargo audit` and `cargo deny` run in CI. Check the current advisory state. |
| Webhooks | Not applicable | No webhooks configured by the repository. |

Scorecard's scores are computed by Scorecard against the public repository. Results can only be
published while the repository is public.

## Best Practices badge

Registration needs a person to log in at <https://www.bestpractices.dev>. The answers below are what
the repository supports. A person must confirm each one.

## Open items (TODO)

- **CODEOWNERS.** `.github/CODEOWNERS` has placeholders `@OWNER-1 @OWNER-2`. Replace them with at least
  two human maintainers. Do not list bot accounts.
- **Default-branch ruleset.** Create a ruleset that requires the `Rust checks` status check, a pull
  request, and approval from a code owner, and that blocks force pushes and deletion. Enabling it
  before the code owners are set would block merges.
- **Security response target** in `SECURITY.md`.
- **Enforcement contact** in `CODE_OF_CONDUCT.md`.
- **Making the repository public.** The Scorecard publication, the badges, and the Best Practices
  registration all need it. Visibility is not changed by this repository's files.
