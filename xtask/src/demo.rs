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
///   - F4 cycles the active window through frameless fullscreen: Desktop (fills
///     the desktop) → Screen (covers the menu, which collapses to a [⋮] kebab) → Off.
///   - F10 → Right Right → Down opens the Windows menu; a=Cascade.
///   - F5 zooms/restores the top window.
fn tour() -> Vec<Scene> {
    vec![
        scene(&[], 110),                  // the desktop
        scene(&["F10"], 60),              // menu bar active
        scene(&["Down"], 130),            // system menu (incl. Color Picker + Splitter)
        scene(&["Enter"], 160),           // About dialog (hero)
        scene(&["Escape"], 50),           // dismiss
        scene(&["F10", "Down", "c"], 90), // Calculator window
        // Click the keypad with the mouse: each press depresses the button
        // (its drop-shadow vanishes), release fires it. 7 × 6 = 42.
        scene(&["\x1b[<0;10;11M"], 30),  // press 7  (button depressed)
        scene(&["\x1b[<0;10;11m"], 35),  // release  → 7
        scene(&["\x1b[<0;25;13M"], 30),  // press ×
        scene(&["\x1b[<0;25;13m"], 35),  // release
        scene(&["\x1b[<0;20;13M"], 30),  // press 6
        scene(&["\x1b[<0;20;13m"], 35),  // release
        scene(&["\x1b[<0;20;17M"], 30),  // press =
        scene(&["\x1b[<0;20;17m"], 120), // release  → 42
        // drag the calculator (size/move mode)
        scene(&["C-F5"], 60),
        scene(&["Left"], 26),
        scene(&["Up"], 26),
        scene(&["Enter"], 90),             // confirm move
        scene(&["F10", "Down", "l"], 100), // Calendar window
        // Color picker — drag across the hue/sat plane; "New" sweeps live. The
        // picker activates on first click (`first_click`), so a throwaway focus
        // click precedes the press; the press-drag-release then stays inside the
        // SV box (screen cols 14–46, rows 6–21 at this layout) so every move
        // scrubs the saturation/value and the "New" swatch sweeps.
        scene(&["F10", "Down", "k"], 130), // open Color Picker
        scene(&["\x1b[<0;16;7M", "\x1b[<0;16;7m"], 50), // focus click (activates picker)
        scene(&["\x1b[<0;18;8M"], 30),     // press in the SV box
        scene(&["\x1b[<32;28;12M"], 45),   // drag…
        scene(&["\x1b[<32;38;16M"], 45),
        scene(&["\x1b[<32;44;20M"], 45),
        scene(&["\x1b[<0;44;20m"], 120), // release (final picked colour)
        scene(&["F10", "Down", "s"], 150), // Splitter grid (joined panes)
        // The list pane (top-right) is a filtering ListBox: the user types to
        // narrow the visible items in real time. Focus lands on it automatically.
        scene(&["a"], 90),          // filter → items containing "a"
        scene(&["n"], 130),         // filter → "an": Banana, Cranberry, Mango, Orange, Tangerine
        scene(&["BackSpace"], 100), // widen back → "a" rows
        scene(&["Escape"], 110),    // clear query — full list returns
        // Resize the split window interactively: Ctrl-F5 enters resize mode; Tab
        // cycles the target (window → each divider); arrows move the active
        // target; Enter commits. Widen the tree pane, then grow the list pane.
        scene(&["C-F5"], 80),  // enter resize mode (frame highlights)
        scene(&["Tab"], 70),   // target the vertical divider (it glows)
        scene(&["Right"], 50), // drag it right — the tree pane widens…
        scene(&["Right"], 50),
        scene(&["Right"], 60),
        scene(&["Tab"], 70),  // target the horizontal divider
        scene(&["Down"], 50), // grow the list pane…
        scene(&["Down"], 60),
        scene(&["Enter"], 150), // commit the new layout
        // Fullscreen tour on the splitter window: F4 cycles the active window
        // Off → Desktop (frameless, content reflows to fill the desktop) →
        // Screen (also covers the menu row, which collapses to a [⋮] kebab at
        // the top-right) → Off (the framed window returns).
        scene(&["F4"], 150), // Desktop: frameless, fills the desktop
        scene(&["F4"], 170), // Screen: covers the menu, [⋮] kebab top-right
        scene(&["F4"], 120), // restore the framed window
        scene(&["F10", "Right", "Right", "Down", "a"], 170), // cascade (all windows)
        scene(&["F5"], 140), // zoom top window
        scene(&["F5"], 110), // restore
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
            // Keys beginning with ESC are raw SGR mouse sequences — send them
            // literally (`-l`); everything else is a named key for send-keys.
            if key.starts_with('\x1b') {
                tmux(&["send-keys", "-t", session, "-l", key])?;
            } else {
                tmux(&["send-keys", "-t", session, key])?;
            }
            sleep(Duration::from_millis(300));
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
