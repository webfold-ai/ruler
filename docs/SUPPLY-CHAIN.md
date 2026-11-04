# Supply chain

What protects the crate's build and release, and where each control is.

| Control | Where | What it checks |
|---|---|---|
| Locked dependencies | `Cargo.lock`, `fuzz/Cargo.lock`; every CI cargo command uses `--locked` | Builds use exactly the recorded versions. |
| Zero runtime dependencies | `Cargo.toml` | The library has no dependencies. The only dependency tree is `fuzz/` (`libfuzzer-sys`). |
| Advisory scan | `.github/workflows/supply-chain.yml`, job `audit` | `cargo audit --deny warnings` on both lockfiles. |
| Policy (licenses, sources, bans) | `deny.toml`, job `deny` | Permissive licenses only; crates.io is the only registry; no git sources; no wildcard versions. |
| Dependency review on PRs | `.github/workflows/dependency-review.yml` | Blocks a PR that adds a high-severity advisory or a disallowed license. Needs GitHub Advanced Security on a private repository. |
| SBOM | job `sbom` | CycloneDX JSON (`ruler.sbom.json`), kept as an artifact for 90 days. |
| Pinned actions | every `uses:` line | Full commit SHA, with the tag as a comment. |
| Least privilege | top-level `permissions:` in every workflow; `persist-credentials: false` on checkouts | Jobs get only the scopes they need. |
| Dependency updates | `.github/dependabot.yml` | Weekly PRs for cargo (root and `fuzz/`) and GitHub Actions. |
| Signed releases and provenance | `.github/workflows/release.yml`, `docs/RELEASING.md` | Keyless cosign signatures and SLSA build provenance. |
| Static analysis | `.github/workflows/codeql.yml` (Rust, `security-extended`) | Runs on PRs, on the default branch, and weekly. |
| Fuzzing | `.github/workflows/fuzz.yml`, `fuzz/` | Both targets run daily and on PRs that touch code. |
| Scorecard | `.github/workflows/scorecard.yml` | Results are published and uploaded to code scanning. |

## Verifying a release

Replace `v0.1.0` with the tag you are checking:

```bash
sha256sum -c SHA256SUMS
cosign verify-blob \
  --bundle ruler-v0.1.0.crate.sigstore.json \
  --certificate-identity-regexp '^https://github.com/webfold-ai/ruler/\.github/workflows/release\.yml@refs/tags/v0\.1\.0$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ruler-v0.1.0.crate
gh attestation verify ruler-v0.1.0.crate --repo webfold-ai/ruler
```

## Known gaps

- The Cargo registry index is trusted as crates.io. The crate has no vendored or mirrored dependencies.
- Reproducible builds are not claimed. The release is a `cargo package` archive, which is deterministic
  only as far as the toolchain is.
- Scorecard's "Maintained", "Code-Review" and "Contributors" checks depend on history and on people,
  not on files in the repository. See [OPENSSF.md](OPENSSF.md).
