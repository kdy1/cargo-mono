# cargo-mono

Mono repository for cargo.

# Installation

```
cargo install cargo-mono
```

# Usage 

## cargo mono bump

```
cargo mono bump swc_common --breaking
```

This will bump version of swc_common and its dependants.
`--breaking` is optional, and if omitted, only patch (according to semver) of specified crate is bumped.


## cargo mono publish

```
cargo mono publish
```

The command defaults to publishing all **publishable** crates.


### Publishing only some of crates

```
cargo mono publish swc_ecmascript
```

This command will publish dependencies of `swc_ecmascript` first and `swc_ecmascript`.


### When only dependencies are changed

`swc_ecmascript` rexports `swc_ecma_transforms` and `Cargo.toml` of `swc_ecmascript` specifies

```toml
[dependencies]
swc_ecma_transforms = "0.1"
```

When you made a small change to `swc_ecma_transforms` and do not want to change version of `swc_ecmascript`, you can do

```
cargo mono publish --allow-only-deps swc_ecmascript
```
