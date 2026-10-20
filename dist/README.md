`danjamd` built for macOS arm64 (Apple silicon) from this tag, with local paths remapped:

```sh
RUSTFLAGS="--remap-path-prefix=$HOME=/build/home --remap-path-prefix=$PWD=/build/danjam" cargo build --release
cp target/release/danjamd dist/danjamd
```

The Homebrew formula installs this file, so installing needs no Rust toolchain.
