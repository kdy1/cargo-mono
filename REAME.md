# cargo-mono

Mono repository for cargo.

# Installation

```
cargo install cargo-mono
```

# Usage 

## cargo bump

```
cargo bump swc_common --breaking
```

This will bump version of swc_common and its dependants.
`--breaking` is optional, and if omitted, only patch (according to semver) of specified crate is bumped.

