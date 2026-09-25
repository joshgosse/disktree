//! `disktree`: find what is eating a volume, mark it, and remove it.
//!
//! The window opens on a treemap of the scanned root — the home directory
//! unless another path is given — with a breadcrumb bar, a selection line, and a
//! live free-space meter. Marking is non-destructive until the review screen
//! is confirmed.

// A window, not a console program: on Windows, opening it from Explorer or
// the Start menu should not bring a console window along. `main` attaches to
// the console of a terminal it was started from, so `--help` and errors still
// reach one. Ignored elsewhere.
#![windows_subsystem = "windows"]

mod app_menu;
mod appearance;
mod charts;
mod git;
mod marks;
mod palette;
mod settings;
mod state;
#[cfg(test)]
mod tests;
mod themes;
mod treemap_view;
mod ui;
mod views;
mod widgets;

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use disktree_core::scan::ScanOptions;
use gpui_kit::{AppContext as _, WindowOptions, px, size};
use settings::Settings;
use state::Disktree;
use themes::ThemeChoice;

/// What the command line asked for.
#[derive(Debug)]
struct Args {
    root: PathBuf,
    options: ScanOptions,
    depth: u32,
}

const USAGE: &str = "\
disktree — a treemap of what is using your disk

usage: disktree [OPTIONS] [PATH]

arguments:
  PATH              directory to scan (default: the home directory)

The window opens on a treemap of the root, largest first. Space marks the
selected tile, Enter opens it, c reviews the marked list, ? lists every key.

options:
  -a, --apparent-size   measure apparent length instead of allocated blocks
  -l, --follow-links    follow symlinks
  -H, --no-hidden       skip dotfiles and dot-directories (and, on macOS,
                        what Finder hides)
  -D, --disk            scan the whole disk the home directory is on
  -X, --cross-filesystems
                        also measure other disks, network shares and pseudo
                        filesystems mounted below PATH (off by default)
  -d, --depth N         how many levels to draw at once (1-6, default 4)
      --metric files    rank by file count instead of bytes
  -h, --help            show this help
";

const DEFAULT_DEPTH: u32 = 4;

fn main() -> Result<()> {
    #[cfg(windows)]
    console::attach();
    let outcome = run();
    #[cfg(windows)]
    console::detach();
    outcome
}

fn run() -> Result<()> {
    let args = parse_args()?;
    let root = args.root.clone();
    let depth = args.depth;
    let title_root = root.clone();
    let settings = Settings::path()
        .map(|path| Settings::load(&path))
        .unwrap_or_default();

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.on_open_urls(|urls| opened::receive(&urls));
    app.run(move |cx| {
        gpui_omarchy::init(cx);
        app_menu::install(cx);
        let home = std::env::home_dir();
        let native_look = appearance::follows_system(home.as_deref());
        let theme = settings.theme.unwrap_or_default();
        // gpui-omarchy follows the Omarchy theme already.
        if native_look || theme != ThemeChoice::System {
            themes::apply(theme, native_look, cx);
        }
        let options = args.options.clone();
        let root_for_app = root.clone();
        let zoom = settings.zoom;
        let bounds = saved_bounds(&settings, cx).unwrap_or_else(|| {
            gpui_kit::Bounds::new(
                gpui_kit::point(px(120.), px(90.)),
                size(px(1440.), px(900.)),
            )
        });
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                        bounds,
                    )),
                    titlebar: Some(gpui_kit::TitlebarOptions {
                        title: Some(
                            format!(
                                "disktree · {}",
                                marks::display_path(
                                    &title_root,
                                    home.as_deref(),
                                )
                            )
                            .into(),
                        ),
                        // On macOS the top bar is the titlebar, as a Mac
                        // app's toolbar is, with the window buttons inset
                        // into it.
                        appears_transparent: cfg!(target_os = "macos"),
                        traffic_light_position: cfg!(target_os = "macos").then(
                            || {
                                gpui_kit::point(
                                    px(ui::TRAFFIC_LIGHTS_X),
                                    px(ui::TRAFFIC_LIGHTS_Y),
                                )
                            },
                        ),
                    }),
                    // Wayland app id. Hyprland reports it as the window
                    // class, and the desktop entry's StartupWMClass and
                    // the documented window rule both match `disktree`.
                    // Left unset, the class is empty and that rule never
                    // matches.
                    app_id: Some("disktree".to_owned()),
                    // Below this the treemap stops being readable, so ask
                    // the compositor not to go there.
                    window_min_size: Some(size(px(900.), px(600.))),
                    ..Default::default()
                },
                move |window, cx| {
                    appearance::follow(window);
                    if let Some(zoom) = zoom {
                        window.set_rem_size(px(ui::BASE_REM * zoom));
                    }
                    cx.new(|cx| {
                        let app = Disktree::new(
                            root_for_app.clone(),
                            options.clone(),
                            depth,
                            cx,
                        );
                        cx.observe_window_bounds(window, |this, window, _| {
                            this.remember_frame(window);
                        })
                        .detach();
                        cx.on_app_quit(|this, _| {
                            this.save_settings();
                            async {}
                        })
                        .detach();
                        opened::follow(cx);
                        app
                    })
                },
            )
            .expect("open the disktree window");

        // The treemap owns the keyboard from the first frame; there is no
        // text field to focus first.
        let _ = window.update(cx, |this, window, cx| {
            let focus = this.focus.clone();
            window.focus(&focus, cx);
        });
        cx.activate(true);
    });
    Ok(())
}

/// The frame the window had when disktree last quit, if it still lands on
/// a display. Only where an app places its own window: a tiling compositor
/// decides for itself.
fn saved_bounds(
    settings: &Settings,
    cx: &gpui_kit::App,
) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    if !cfg!(any(target_os = "macos", windows)) {
        return None;
    }
    let frame = settings.frame?;
    let bounds = gpui_kit::Bounds::new(
        gpui_kit::point(px(frame.x), px(frame.y)),
        size(px(frame.width.max(900.)), px(frame.height.max(600.))),
    );
    cx.displays()
        .iter()
        .any(|display| display.bounds().intersects(&bounds))
        .then_some(bounds)
}

/// Folders handed to the running app: Finder's Open With, a folder dropped
/// on the Dock icon. They arrive as `file://` URLs, before or after the
/// window exists.
mod opened {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    use gpui_kit::Context;

    use crate::state::Disktree;

    static WAITING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    pub fn receive(urls: &[String]) {
        let folders = urls
            .iter()
            .filter_map(|url| path_of(url))
            .filter(|path| path.is_dir());
        WAITING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(folders);
    }

    /// Scan the newest folder handed over, whenever one is.
    pub fn follow(cx: &Context<'_, Disktree>) {
        if !cfg!(target_os = "macos") {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let folder = WAITING
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .drain(..)
                    .next_back();
                let alive = this
                    .update(cx, |this, cx| {
                        if let Some(folder) = folder {
                            this.open_path(folder, cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    /// The path a `file://` URL names, with its escapes undone.
    pub fn path_of(url: &str) -> Option<PathBuf> {
        let rest = url.strip_prefix("file://")?;
        // A host, if any, precedes the path; only this machine's is served.
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        let bytes = path.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%'
                && let Some(byte) = bytes
                    .get(index + 1..index + 3)
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                decoded.push(byte);
                index += 3;
            } else {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
        let text = String::from_utf8(decoded).ok()?;
        let trimmed = text.trim_end_matches('/');
        Some(PathBuf::from(if trimmed.is_empty() {
            "/"
        } else {
            trimmed
        }))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_file_url_names_its_path_with_escapes_undone() {
            assert_eq!(
                path_of("file:///Users/tobi/My%20Folder/"),
                Some(PathBuf::from("/Users/tobi/My Folder"))
            );
            assert_eq!(
                path_of("file://localhost/tmp/a%2Fb"),
                Some(PathBuf::from("/tmp/a/b"))
            );
            assert_eq!(path_of("file:///"), Some(PathBuf::from("/")));
            assert_eq!(path_of("https://example.com/x"), None);
            assert_eq!(path_of("file://server/share"), None);
        }
    }
}

fn parse_args() -> Result<Args> {
    let mut root: Option<PathBuf> = None;
    let mut options = ScanOptions::default();
    let mut depth = DEFAULT_DEPTH;
    let mut disk = false;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "-a" | "--apparent-size" => options.apparent_size = true,
            "-l" | "--follow-links" => options.follow_links = true,
            "-H" | "--no-hidden" => options.include_hidden = false,
            // Staying on one volume is the default; the flag is kept so
            // old invocations still work.
            "-x" | "--one-filesystem" => options.one_filesystem = true,
            "-X" | "--cross-filesystems" => options.one_filesystem = false,
            "-D" | "--disk" => disk = true,
            "-d" | "--depth" => {
                let value = args.next().context("--depth needs a number")?;
                depth = value.parse().context("--depth needs a number")?;
                anyhow::ensure!(
                    (1..=6).contains(&depth),
                    "--depth must be 1 to 6"
                );
            }
            "--metric" => {
                let value = args.next().context("--metric needs a value")?;
                options.metric = match value.as_str() {
                    "files" => disktree_core::tree::Metric::Files,
                    "bytes" | "size" => disktree_core::tree::Metric::Bytes,
                    other => anyhow::bail!(
                        "unknown metric {other}; try bytes or files"
                    ),
                };
            }
            // Launch Services added a process serial number when opening an
            // app from Finder until OS X 10.9, and some launchers still do.
            other if other.starts_with("-psn_") => {}
            // Xcode and Launch Services pass their own defaults, each with a
            // value: `-NSDocumentRevisionsDebugMode YES`.
            other
                if other.starts_with("-NS") || other.starts_with("-Apple") =>
            {
                args.next();
            }
            other if other.starts_with('-') => {
                anyhow::bail!("unknown option {other}\n\n{USAGE}");
            }
            path => {
                anyhow::ensure!(root.is_none(), "only one path can be scanned");
                root = Some(PathBuf::from(path));
            }
        }
    }

    anyhow::ensure!(
        !(disk && root.is_some()),
        "--disk and a PATH cannot be combined"
    );
    let home = std::env::home_dir();
    let root = match root {
        _ if disk => home
            .as_deref()
            .and_then(disktree_core::space::volume_root_for)
            .unwrap_or_else(|| PathBuf::from("/")),
        Some(root) => root,
        None => home.context("no path given and no home directory")?,
    };
    // Store the depth as the initial view setting rather than a scan option: it
    // is a display choice the run-time `[` and `]` keys also change.
    // Canonical, so a later widening recognises this tree in the wider walk;
    // through dunce, so Windows gets `C:\Users\…` rather than the `\\?\C:\…`
    // form nothing else is written in.
    let root = dunce::canonicalize(&root).unwrap_or(root);
    let metadata = std::fs::metadata(&root)
        .with_context(|| format!("cannot read {}", root.display()))?;
    anyhow::ensure!(metadata.is_dir(), "{} is not a directory", root.display());

    Ok(Args {
        root,
        options,
        depth: depth.clamp(1, 6),
    })
}

/// The console of the terminal disktree was started from, if any: a
/// windowed program on Windows gets none of its own.
#[cfg(windows)]
mod console {
    #![allow(
        unsafe_code,
        reason = "two Win32 calls that take no pointers to get wrong"
    )]

    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, FreeConsole,
    };

    /// Borrow the parent's console so printed text reaches it. Does
    /// nothing when started from Explorer, which has none.
    pub fn attach() {
        // SAFETY: takes a process id by value, and failure only means
        // there was no console to attach to.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    /// Let go of it again, so the shell redraws its prompt.
    pub fn detach() {
        // SAFETY: no arguments; a process without a console is left as is.
        unsafe {
            FreeConsole();
        }
    }
}
