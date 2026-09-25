//! The menu bar and the application-wide shortcuts.
//!
//! GPUI adds neither on its own. On macOS an app without them has an empty
//! menu bar and no ⌘Q, so this is where disktree becomes a Mac app. GPUI
//! draws no menu bar on Linux or Windows, so there the same actions get the
//! shortcuts their users expect, with ctrl, and nothing else.

use gpui_kit::{App, KeyBinding, Menu, MenuItem, SystemMenuType};

use crate::charts::Chart;
use crate::themes::ThemeChoice;

pub use menu_actions::*;

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "GPUI's actions! macro writes the derives, not us"
)]
mod menu_actions {
    use gpui_kit::actions;

    actions!(
        disktree,
        [
            /// Quit the application.
            Quit,
            /// Hide the application's windows.
            Hide,
            /// Hide every other application.
            HideOthers,
            /// Show every application again.
            ShowAll,
            /// Close the focused window, which quits: there is only one.
            CloseWindow,
            /// Scan the current root again.
            Rescan,
            /// Choose a directory to scan.
            OpenFolder,
            /// Reveal the tile a key acts on in Finder.
            ShowInFinder,
            /// Back through the directories visited.
            GoBack,
            /// Forward again, after going back.
            GoForward,
            /// Areas by size, coloured by kind.
            ShowSize,
            /// Areas by file count, coloured by kind.
            ShowFiles,
            /// Areas by size, coloured by age.
            ShowAge,
            ShowTreemap,
            ShowSunburst,
            ShowIcicle,
            /// Scan again with hidden entries, or without.
            ToggleHidden,
            /// Scan again measuring apparent size, or disk usage.
            ToggleApparent,
            FilterByName,
            FewerLevels,
            MoreLevels,
            /// Interface zoom one step up.
            ZoomIn,
            /// Interface zoom one step down.
            ZoomOut,
            /// Interface zoom back to 100%.
            ActualSize,
            TogglePanel,
            /// Up one folder.
            EnclosingFolder,
            OpenSelection,
            HomeFolder,
            WholeDisk,
            /// Mark or unmark the tile a key acts on.
            ToggleMark,
            ReviewMarks,
            UnmarkAll,
            /// The keyboard and mouse overlay.
            KeyboardShortcuts,
            OpenRepository,
            ThemeSystem,
            ThemeHighContrast,
            ThemeLight,
            ThemePaper,
            ThemeGlacier,
            ThemeDark,
            ThemeMidnight,
            ThemeForest,
            ThemeEmber,
        ]
    );
}

/// What the menus show a check beside, so they are rebuilt only when one
/// of these changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuState {
    pub mode: usize,
    pub chart: Chart,
    pub hidden: bool,
    pub apparent: bool,
    pub panel: bool,
    pub theme: ThemeChoice,
}

pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Install the menus, the shortcuts and the quit-on-close rule.
pub fn install(cx: &mut App) {
    // A single-window utility has nothing left to show once its window is
    // gone; staying alive would leave a Dock icon with no window behind it.
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    });
    // `Rescan`, `OpenFolder`, `ShowInFinder` and the history are handled by
    // the window, which owns the scan and the selection; see `views::root`.
    if !cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("ctrl-w", CloseWindow, None),
            KeyBinding::new("ctrl-o", OpenFolder, None),
            KeyBinding::new("ctrl-r", Rescan, None),
            KeyBinding::new("f5", Rescan, None),
            KeyBinding::new("ctrl-shift-r", ShowInFinder, None),
        ]);
        return;
    }
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-r", Rescan, None),
        KeyBinding::new("cmd-o", OpenFolder, None),
        // ⌘R is taken by Rescan, as in a browser; Xcode uses ⌘⇧R-like
        // chords for its reveals, and `o` does the same without a modifier.
        KeyBinding::new("cmd-shift-r", ShowInFinder, None),
        // Finder's and Safari's back and forward; alt-arrows work too.
        KeyBinding::new("cmd-[", GoBack, None),
        KeyBinding::new("cmd-]", GoForward, None),
        KeyBinding::new("cmd-1", ShowSize, None),
        KeyBinding::new("cmd-2", ShowFiles, None),
        KeyBinding::new("cmd-3", ShowAge, None),
        // Finder's own shortcut for hidden files.
        KeyBinding::new("cmd-shift-.", ToggleHidden, None),
        KeyBinding::new("cmd-f", FilterByName, None),
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-0", ActualSize, None),
        KeyBinding::new("alt-cmd-i", TogglePanel, None),
        KeyBinding::new("cmd-up", EnclosingFolder, None),
        KeyBinding::new("cmd-down", OpenSelection, None),
        KeyBinding::new("cmd-shift-h", HomeFolder, None),
        KeyBinding::new("cmd-shift-d", WholeDisk, None),
        KeyBinding::new("cmd-backspace", ToggleMark, None),
        KeyBinding::new("cmd-e", ReviewMarks, None),
        KeyBinding::new("cmd-/", KeyboardShortcuts, None),
    ]);
    cx.on_action(|_: &OpenRepository, cx| cx.open_url(REPOSITORY));
    set_menus(
        MenuState {
            mode: 0,
            chart: Chart::default(),
            hidden: false,
            apparent: false,
            panel: true,
            theme: ThemeChoice::default(),
        },
        cx,
    );
}

/// The menu bar, with checks beside what `state` says is on. macOS only:
/// elsewhere GPUI draws no menu bar.
pub fn set_menus(state: MenuState, cx: &App) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let check = |item: MenuItem, on: bool| item.checked(on);
    let theme = |choice: ThemeChoice| {
        let item = match choice {
            ThemeChoice::System => {
                MenuItem::action(choice.label(), ThemeSystem)
            }
            ThemeChoice::HighContrast => {
                MenuItem::action(choice.label(), ThemeHighContrast)
            }
            ThemeChoice::Light => MenuItem::action(choice.label(), ThemeLight),
            ThemeChoice::Paper => MenuItem::action(choice.label(), ThemePaper),
            ThemeChoice::Glacier => {
                MenuItem::action(choice.label(), ThemeGlacier)
            }
            ThemeChoice::Dark => MenuItem::action(choice.label(), ThemeDark),
            ThemeChoice::Midnight => {
                MenuItem::action(choice.label(), ThemeMidnight)
            }
            ThemeChoice::Forest => {
                MenuItem::action(choice.label(), ThemeForest)
            }
            ThemeChoice::Ember => MenuItem::action(choice.label(), ThemeEmber),
        };
        check(item, state.theme == choice)
    };
    let mut themes = Vec::new();
    for (index, choice) in ThemeChoice::ALL.into_iter().enumerate() {
        // Following the system, then the light themes, then the dark.
        if index == 1 || choice == ThemeChoice::Dark {
            themes.push(MenuItem::separator());
        }
        themes.push(theme(choice));
    }
    cx.set_menus([
        Menu::new("disktree").items([
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide disktree", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit disktree", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Open Folder\u{2026}", OpenFolder),
            MenuItem::action("Show in Finder", ShowInFinder),
            MenuItem::separator(),
            MenuItem::action("Rescan", Rescan),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("View").items([
            check(MenuItem::action("Size", ShowSize), state.mode == 0),
            check(MenuItem::action("Files", ShowFiles), state.mode == 1),
            check(MenuItem::action("Age", ShowAge), state.mode == 2),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new("Chart").items([
                check(
                    MenuItem::action("Treemap", ShowTreemap),
                    state.chart == Chart::Treemap,
                ),
                check(
                    MenuItem::action("Sunburst", ShowSunburst),
                    state.chart == Chart::Sunburst,
                ),
                check(
                    MenuItem::action("Icicle", ShowIcicle),
                    state.chart == Chart::Icicle,
                ),
            ])),
            MenuItem::separator(),
            check(
                MenuItem::action("Show Hidden Files", ToggleHidden),
                state.hidden,
            ),
            check(
                MenuItem::action("Apparent Size", ToggleApparent),
                state.apparent,
            ),
            MenuItem::separator(),
            MenuItem::action("Filter by Name\u{2026}", FilterByName),
            MenuItem::action("Fewer Levels", FewerLevels),
            MenuItem::action("More Levels", MoreLevels),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ActualSize),
            MenuItem::separator(),
            MenuItem::action(
                if state.panel {
                    "Hide Side Panel"
                } else {
                    "Show Side Panel"
                },
                TogglePanel,
            ),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new("Theme").items(themes)),
        ]),
        Menu::new("Go").items([
            MenuItem::action("Back", GoBack),
            MenuItem::action("Forward", GoForward),
            MenuItem::separator(),
            MenuItem::action("Enclosing Folder", EnclosingFolder),
            MenuItem::action("Open Selection", OpenSelection),
            MenuItem::separator(),
            MenuItem::action("Home Folder", HomeFolder),
            MenuItem::action("Whole Disk", WholeDisk),
        ]),
        Menu::new("Marks").items([
            MenuItem::action("Mark or Unmark Selection", ToggleMark),
            MenuItem::action("Review Marked Items\u{2026}", ReviewMarks),
            MenuItem::separator(),
            MenuItem::action("Unmark All", UnmarkAll),
        ]),
        Menu::new("Help").items([
            MenuItem::action("Keyboard Shortcuts", KeyboardShortcuts),
            MenuItem::action("disktree on GitHub", OpenRepository),
        ]),
    ]);
}
