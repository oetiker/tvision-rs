//! rstv documentation build tool. Pure-cargo entry point: `cargo xtask <cmd>`.

mod ansi_html;
mod build;
mod demo;
mod linkcheck;
mod paths;
mod raster;
mod screens;
mod serve;
mod test;

use anyhow::Result;

fn usage() -> ! {
    eprintln!(
        "cargo xtask <command>\n\
         \n\
         commands:\n\
         \x20 docs [--serve]   build the integrated doc site (guide + api); --serve = watch+serve\n\
         \x20 screens          regenerate the tmux screenshots only\n\
         \x20 test             run mdbook doctests (compile the guide's rust blocks)\n\
         \x20 demo             record docs/demo/tvdemo.webp (drive tvdemo in tmux, rasterize frames)\n"
    );
    std::process::exit(2)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("docs") => {
            let serve = args.iter().any(|a| a == "--serve");
            if serve { serve::run() } else { build::docs() }
        }
        Some("screens") => screens::regenerate(),
        Some("test") => test::run(),
        Some("demo") => demo::run(),
        _ => usage(),
    }
}
