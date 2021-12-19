# cargo-mono

Mono repository for cargo.

# Installation

```
cargo install cargo-mono
```

# Usage

## cargo mono bump (interactive)

```
cargo mono bump -i
```

## cargo mono bump

```
cargo mono bump swc_common --breaking
```

This will bump version of swc_common and its dependants.
`--breaking` is optional, and if omitted, only patch (according to semver) of specified crate is bumped.

Even if it's not a breaking change, you may want to bump dependants along with it.
If so, you can use `-D` like

```
cargo mono bump swc_common -D
```

The command above will bump version of swc_common and its dependants. Requirements of dependants packages will be updated too.

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
