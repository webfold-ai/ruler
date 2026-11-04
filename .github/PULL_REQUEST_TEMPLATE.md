## Summary

<!-- What changes, and why. -->

## Type of change

- [ ] Bug fix (result or panic)
- [ ] New feature or API
- [ ] Documentation only
- [ ] Build, CI, or supply chain

## Checks

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --locked`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `cargo deny check advisories bans licenses sources` (if dependencies changed)
- [ ] Layout result changes are covered by the naive reference in `tests/contracts.rs`
- [ ] No `unsafe` code added

## Notes for reviewers

<!-- Anything a reviewer should look at first. Mention any new dependency and why it is needed. -->
