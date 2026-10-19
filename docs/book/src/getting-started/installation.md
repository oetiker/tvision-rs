# Installation & the `tv::` alias

## Add the crate

`tvision-rs` targets a recent stable Rust (the crate uses the 2024 edition, so you
need Rust 1.85 or newer). Add it to your `Cargo.toml`. The house style is to
alias the package to `tv`, so import it under that name with Cargo's
`package` key:

```toml
[dependencies]
tv = { package = "tvision-rs", version = "0.18.0" }
```

To track the development version on `main`, depend on the git repository
instead:

```toml
[dependencies]
tv = { package = "tvision-rs", git = "https://github.com/oetiker/tvision-rs" }
```

Everything is then reachable through the `tv::` namespace, which replaces the
old `T` prefix:

```rust
# #![allow(unused_imports)]
# use tvision_rs as tv;
use tv::{Program, Desktop, MenuBar, StatusLine, CrosstermBackend};
```

The `tv` alias is a convention; `use tvision_rs::…` works as well.

## Feature flags

| Feature        | Default | What it does                                                                 |
| -------------- | ------- | ---------------------------------------------------------------------------- |
| `os-clipboard` | **on**  | Native OS clipboard (via `arboard`) as the first rung of the clipboard chain |

The clipboard is a fall-through chain: native OS clipboard → terminal OSC 52
escape → an internal in-process buffer. Turning `os-clipboard` **off** drops the
`arboard` dependency (and its system libraries) but still leaves the OSC 52 and
internal rungs, so copy/paste keeps working on capable terminals:

```toml
[dependencies]
tv = { package = "tvision-rs", version = "0.18.0", default-features = false }
```

The git dependency takes the same `default-features = false` key.

## Verify the build

A build confirms that the crate resolves and compiles for your toolchain:

```console
$ cargo build
   Compiling tvision-rs vX.Y.Z
   Compiling my-app v0.1.0 (/home/me/my-app)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.34s
```

`vX.Y.Z` is the `tvision-rs` version Cargo resolved; for a git dependency
Cargo also prints the repository URL and commit.

Next: [your first app](first-app.md).
