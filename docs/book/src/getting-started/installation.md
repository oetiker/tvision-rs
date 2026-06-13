# Installation & the `tv::` alias

## Add the crate

`tvision` targets a recent stable Rust (the crate uses the 2024 edition, so you
need Rust 1.85 or newer). Add it to your `Cargo.toml`. The house style is to
alias the package to `tv`, so import it under that name with Cargo's
`package` key:

```toml
[dependencies]
tv = { package = "tvision", git = "https://github.com/oetiker/rstv" }
```

Once a release is published to crates.io you will be able to pin a version
instead:

```toml
[dependencies]
tv = { package = "tvision", version = "0.1" }
```

Now everything is reachable through the `tv::` namespace — the path is the
namespace the old `T` prefix was faking:

```rust
# #![allow(unused_imports)]
# use tvision as tv;
use tv::{Program, Desktop, MenuBar, StatusLine, CrosstermBackend};
```

If you prefer the crate's own name, you can of course `use tvision::…` directly;
the `tv` alias is a convention, not a requirement.

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
tv = { package = "tvision", git = "https://github.com/oetiker/rstv", default-features = false }
```

## Verify the build

A plain build is enough to confirm the crate resolves and compiles for your
toolchain:

```console
$ cargo build
   Compiling tvision v0.1.0
    Finished `dev` profile [unoptimized + debuginfo]
```

With the crate in place, you are ready to write [your first
app](first-app.md).
