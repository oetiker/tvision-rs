//! `cargo xtask demo` — record an animated WebP of `tvdemo` with no external
//! recorder. Drives the example in a detached tmux session (the same mechanism
//! as the screenshots), captures one coloured frame per scene
//! (`tmux capture-pane -e -p -N`), rasterizes it (`raster.rs`) and encodes an
//! animated WebP (`webp` crate). Fully owned + deterministic, like `ansi_html`.

use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use webp::{AnimEncoder, AnimFrame, WebPConfig};

use crate::{ansi_html, paths, raster};

const COLS: u32 = 90;
const ROWS: u32 = 28;

/// One scene: keystrokes to send (each via its own `send-keys`, then a settle),
/// and how long to hold the resulting frame, in centiseconds.
struct Scene {
    keys: &'static [&'static str],
    hold_cs: u16,
}

const fn scene(keys: &'static [&'static str], hold_cs: u16) -> Scene {
    Scene { keys, hold_cs }
}

/// The tour, validated against tvdemo:
///   - F10 activates the menu bar; Down opens the system (≡) menu; Enter = About.
///   - In the open system menu the item hotkey opens it: l=Calendar, c=Calculator,
///     t=Ascii table.
///   - The calculator takes keyboard input (6 * 7 = → 42).
///   - Ctrl-F5 enters size/move mode; arrows glide the window; Enter confirms.
///   - F10 → Right Right → Down opens the Windows menu; a=Cascade.
///   - F5 zooms/restores the top window.
fn tour() -> Vec<Scene> {
    vec![
        scene(&[], 120),                   // the desktop
        scene(&["F10"], 70),               // menu bar active
        scene(&["Down"], 90),              // system menu open
        scene(&["Enter"], 180),            // About dialog (hero)
        scene(&["Escape"], 50),            // dismiss
        scene(&["F10", "Down", "l"], 110), // Calendar window
        scene(&["F10", "Down", "c"], 110), // Calculator window
        // press buttons: 6 * 7 = → 42
        scene(&["6"], 40),
        scene(&["*"], 40),
        scene(&["7"], 40),
        scene(&["="], 150),
        // drag the calculator around (size/move mode)
        scene(&["C-F5"], 70),
        scene(&["Left"], 30),
        scene(&["Left"], 30),
        scene(&["Up"], 30),
        scene(&["Enter"], 100),                              // confirm move
        scene(&["F10", "Down", "t"], 110),                   // Ascii-table window
        scene(&["F10", "Right", "Right", "Down", "a"], 170), // cascade (hero)
        scene(&["F5"], 150),                                 // zoom top window
        scene(&["F5"], 110),                                 // restore
    ]
}

fn tmux(args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("tmux")
        .args(args)
        .output()
        .context("spawn tmux")?;
    anyhow::ensure!(out.status.success(), "tmux {:?} failed", args);
    Ok(out.stdout)
}

pub fn run() -> Result<()> {
    // Build the example up front so each launch is instant and stable.
    let status = Command::new("cargo")
        .args(["build", "--release", "--example", "tvdemo", "-j4"])
        .current_dir(paths::workspace_root())
        .status()
        .context("build tvdemo")?;
    anyhow::ensure!(status.success(), "building tvdemo failed");

    let bin = paths::target_dir()
        .join("release")
        .join("examples")
        .join("tvdemo");
    anyhow::ensure!(bin.exists(), "tvdemo binary missing at {}", bin.display());

    let session = "rstv_demo";
    let _ = tmux(&["kill-session", "-t", session]);
    let launch = format!("'{}'; tmux wait-for -S demodone", bin.display());
    tmux(&[
        "new-session",
        "-d",
        "-s",
        session,
        "-x",
        &COLS.to_string(),
        "-y",
        &ROWS.to_string(),
        "bash",
        "-lc",
        &launch,
    ])
    .context("tmux new-session")?;

    let renderer = raster::Renderer::new();
    let scenes = tour();
    // Keep the rendered RGBA buffers owned for the whole encode (AnimFrame borrows them).
    let mut images: Vec<(image::RgbaImage, u16)> = Vec::with_capacity(scenes.len());

    sleep(Duration::from_millis(900)); // settle the first paint
    let dump_dir = paths::workspace_root().join("target").join("demo-frames");
    let _ = std::fs::create_dir_all(&dump_dir);

    for (i, sc) in scenes.iter().enumerate() {
        for key in sc.keys {
            tmux(&["send-keys", "-t", session, key])?;
            sleep(Duration::from_millis(320));
        }
        sleep(Duration::from_millis(420));
        let captured = tmux(&["capture-pane", "-t", session, "-e", "-p", "-N"])?;
        let grid = ansi_html::parse_grid(&String::from_utf8_lossy(&captured));
        let img = renderer.render(&grid, COLS, ROWS);
        let _ = img.save(dump_dir.join(format!("scene{i:02}.png")));
        images.push((img, sc.hold_cs));
        eprintln!("  scene {i:02} captured");
    }

    let _ = tmux(&["send-keys", "-t", session, "M-x"]); // quit tvdemo
    let _ = tmux(&["kill-session", "-t", session]);

    // Encode animated WebP. libwebp wants each frame's timestamp to be its
    // cumulative START time (frame 0 at 0); the last frame's duration is set by
    // a terminal marker. The `webp` crate hardcodes that marker to t=0, so we
    // append a trailing DUPLICATE of the final frame at the total time — that
    // gives the real last frame its full hold; the (identical) duplicate then
    // collapses to ~0ms, invisibly.
    let (w, h) = images[0].0.dimensions();
    let mut config = WebPConfig::new().map_err(|_| anyhow::anyhow!("WebPConfig::new failed"))?;
    config.lossless = 1; // crisp text, no block artefacts
    let mut encoder = AnimEncoder::new(w, h, &config);
    encoder.set_loop_count(0); // loop forever

    let mut starts = Vec::with_capacity(images.len());
    let mut t_ms: i32 = 0;
    for (_, hold_cs) in &images {
        starts.push(t_ms);
        t_ms += (*hold_cs as i32) * 10;
    }
    for ((img, _), start) in images.iter().zip(&starts) {
        encoder.add_frame(AnimFrame::from_rgba(img.as_raw(), w, h, *start));
    }
    let last = &images.last().unwrap().0;
    encoder.add_frame(AnimFrame::from_rgba(last.as_raw(), w, h, t_ms));
    let webp = encoder.encode();

    let out_path = paths::workspace_root()
        .join("docs")
        .join("demo")
        .join("tvdemo.webp");
    std::fs::create_dir_all(out_path.parent().unwrap()).ok();
    std::fs::write(&out_path, &*webp).context("write webp")?;

    eprintln!(
        "OK: wrote {} ({} scenes, {}x{}px, {:.1}s loop); PNGs in {}",
        out_path.display(),
        images.len(),
        w,
        h,
        t_ms as f32 / 1000.0,
        dump_dir.display()
    );
    Ok(())
}
