# Releasing

The crate is not published to crates.io (`publish = false`). A release is a version tag, and its
artifact is the `cargo package` archive, signed and attested.

## Steps

1. Update `version` in `Cargo.toml` and move the `[Unreleased]` entries in `CHANGELOG.md` under the new
   version heading.
2. Run the checks in `CONTRIBUTING.md` on the commit you will tag.
3. Merge the release commit to `main` through a reviewed pull request.
4. Tag it and push the tag. The tag must be `v` plus the version in `Cargo.toml`:

   ```bash
   git tag -s v0.1.0 -m "v0.1.0"
   git push origin v0.1.0
   ```

5. `.github/workflows/release.yml` runs. It checks the tag against the version, runs the tests,
   packages the crate, signs it and attests it, and creates the GitHub release.
6. Check the release with the commands in [SUPPLY-CHAIN.md](SUPPLY-CHAIN.md#verifying-a-release).

The workflow does not run for a tag that does not match the version. Fix the version or the tag and
push again.
