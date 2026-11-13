# Security policy

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x (current, unpublished) | Yes. Security fixes land on the default branch. |

The crate has not been published. Once a version is released, this table will say which releases
receive fixes.

## Reporting a vulnerability

Please report security problems privately. Do **not** open a public issue or pull request.

Use GitHub's private vulnerability reporting for this repository, or email the security team at security@webfold.ai:
<https://github.com/webfold-ai/ruler/security/advisories/new>

Include:

- the version or commit,
- a minimal input that triggers the problem (for example, text plus a font size and width),
- what you expected and what happened (panic, unbounded memory, wrong layout result, or other).

**Response target:** TODO (maintainers to confirm). We intend to acknowledge a report within a few
business days and to share a plan after triage.

## Scope

In scope:

- panics, out-of-bounds access or undefined behaviour reachable from public API with untrusted input,
- memory use that grows without bound from a small input (see the memory limit in README.md),
- results that violate a documented contract, such as a line that breaks outside the box range or a
  caret outside its line.

Out of scope:

- layout quality (greedy breaking is documented in README.md),
- inaccurate font metrics for characters or faces that are documented as unsupported,
- issues in the caller's use of a large font size or width, unless the crate's own documented limit
  is wrong.

## Supply chain

Dependency advisories are checked by `cargo audit` and `cargo deny` in CI. Releases are signed and
carry build provenance; see [docs/RELEASING.md](docs/RELEASING.md) and
[docs/SUPPLY-CHAIN.md](docs/SUPPLY-CHAIN.md).
