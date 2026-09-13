# Vendored Cargo Patches

This directory contains source patches that cannot be expressed through normal
Cargo feature selection. It must not contain references to repositories outside
this project.

## moq-native 0.19.9

- Upstream: `https://crates.io/crates/moq-native/0.19.9`
- Crate SHA-256: `e559090c9cdeaa4ad8d76ad3755647bec0d85503b21e81ee5022f1da2ba7d33d`
- Upstream commit: `f05f1c4ae24591d507209a8ec4799c393adfaf0b`
- License: MIT OR Apache-2.0; the vendored copy includes the upstream
  `LICENSE-MIT`.

All source files are byte-for-byte identical to the published crate. Only the
normalized `Cargo.toml` is patched:

1. The `ring` feature is propagated to optional `web-transport-quinn`.
2. `web-transport-quinn` default features are disabled.
3. `rustls` and `rustls-webpki` use `ring` without their default `aws-lc-rs`
   provider.

Without this patch, enabling `moq-native/ring` still activates
`web-transport-quinn`'s default `aws-lc-rs` provider. The upstream 0.19.17
release still has that feature wiring, so the patch remains necessary.

`web-transport-quinn` itself is not vendored. Version 0.11.12 comes directly
from crates.io and receives its `ring` feature through this patch.

When upgrading `moq-native`, compare the published crate with this directory,
reapply only the manifest changes above, update the provenance fields, and run:

```bash
cargo tree --locked --features voice-workers -e features
cargo test --locked --all-targets --features voice-workers
```

Remove this patch once upstream propagates `ring` to `web-transport-quinn` and
sets that optional dependency to `default-features = false`.
