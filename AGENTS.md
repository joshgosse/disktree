# disktree — agent guide

A GPUI + gpui-omarchy treemap explorer for disk usage on Omarchy. Read
`README.md` for the product; this file is the working contract.

## What this is

Find what is eating a volume, mark paths for removal, review the list, and
remove it — with the volume's free space live on screen the whole time. Two
phases: explore (treemap, breadcrumbs, the selection and status lines) and review (the marked list,
the removal mode, the confirmation). Marking is never destructive.

## Commands

```sh
make build                      # release build
make run                        # build and run, scanning $HOME
make install                    # ~/.local: binary, desktop entry, icon
make install PREFIX=/usr/local  # system-wide (needs root)
make uninstall
make bundle                     # macOS: target/bundle/disktree.app and its zip
make lint                       # rustfmt --check, then clippy --all-targets -D warnings
make test                       # core and window-harness tests
make ci                         # lint, then test
make fmt                        # format in place
```

`make install` is the supported way to put this on a machine: it installs the
release binary, `packaging/disktree.desktop.in` (rendered with the real install
prefix and the crate version) and `assets/disktree.svg`. Keep the desktop
entry's `Categories` to a single main category plus additional ones, or
`desktop-file-validate` complains.

On macOS `make install` installs `target/bundle/disktree.app` into
`~/Applications` instead, built by `cargo xtask bundle` from
`packaging/macos/` (the `Info.plist` template and the icon's SVG). The bundle
identifier there keys every macOS permission grant; do not change it lightly.

`cargo xtask lint` is the gate. It must be green before anything is called
done, and it must not fix anything: a red local run is the same signal CI gives.
On Windows, where there is no make, run `cargo xtask lint`, `cargo xtask test`
and `cargo build --release` directly; CI runs the gate on both systems.

## House rules

* **Strict lints from omatrack's style.** `clippy::all` and `clippy::pedantic`
  are errors, `clippy::nursery` warns, and `-D warnings` promotes the rest. Every
  deliberate exception is listed with its reason in the workspace `Cargo.toml`
  or as an `#[allow(..., reason = "...")]` on the item — never silently.
* **80 columns**, 4-space indent, by `rustfmt.toml`. Only stable rustfmt
  options, because the gate runs on stable; unstable options would be ignored
  with a warning instead of applying.
* **Comments say why.** The code says what. Any non-obvious number, ordering or
  boundary deserves the reason next to it.
* **Tests live beside the promise they make.** `disktree-core` tests size
  accounting, layout and deletion guards against real temporary trees;
  `disktree-app/src/tests.rs` drives the real window harness — draw a frame,
  press keys — so a screen that panics while painting fails a test.
* **Never delete anything outside a marked path.** See `removal.rs`; the guards
  are load-bearing and are covered by tests.

## Invariants

1. **Sizes come from `st_blocks * 512` unless apparent size was asked for.**
   That is the number that comes back when a file is deleted. On Windows it
   is the allocation the directory listing reports; see `windows.rs`.
2. **`own_bytes`/`own_files` are derived, never tracked.** `tree::aggregate`
   computes the totals from the children. Hardlink de-duplication rewrites a
   leaf's weight and re-aggregates; anything that patches `bytes` directly will
   be overwritten.
3. **A directory is only built when its own scan *and* every subdirectory task
   has finished.** That is the `+1` sentinel in `PendingDir::pending`. Building
   early silently drops whole subtrees — it has happened once.
4. **Only paths under the scanned root may be removed**, and mount points, the
   root, the home directory, any directory holding it, and symlink targets are
   refused.
5. **Marks are keyed by absolute path**, not tree position, so they survive a
   re-scan; `Marks::refresh` re-reads their sizes and drops what is gone.
6. **The treemap is painted, not composed of elements.** Thousands of
   rectangles belong in one canvas callback; labels are shaped there too so they
   clip to their own tile.
7. **Tile crumbs are absolute.** `treemap::layout` takes the drawn node's
   crumbs and every tile extends them, so a tile resolves from the scanned
   root at any depth. Relative crumbs look right at `~` and silently point
   at other directories after descending — including for marks. Any code that
   turns a path into crumbs walks from the scanned root, too.
8. **The view transform is the only thing zoom changes.** Layout runs in
   base-space pixels and is cached; `screen = (base - origin) * scale`.
9. **The status bar never claims a saving it cannot measure.** Projections come
   from marked bytes; the final number comes from `statvfs` before and after.
10. **A "+N more" tile's crumbs are its directory's plus `treemap::REST`.**
    They name no node, so nothing that resolves crumbs may treat them as the
    directory: `rest_at` reads what the tile stands for, it opens as a page
    (`rest_pages`), and it is never marked whole.
11. **Painting and hit-testing a chart share one geometry.** A sunburst and
    an icicle lay out in unit space (`partition`), and `ChartGeometry` alone
    maps that to the screen, both ways.
12. **A level change is drawn, never waited on.** `LevelTransition` moves
    the old level and the new one through the camera while the state has
    already changed; input that would change level again is dropped until
    it has played, and Reduce Motion skips it.
13. **Git is only read.** `git::Git` runs every command with no inherited
    `GIT_*` variable, optional locks off and every program a checkout's
    config could name overridden; nothing fetches, an fsmonitor daemon is
    used only if one is already running, and a filter driver runs only when
    the user's own config defines it, setting for setting; one only the
    checkout defines is switched off for `status`. Removing a linked worktree's folder leaves
    its branches, commits and stashes in its repository, which is why
    `Checkout::loses_nothing` asks only what the folder itself holds.
14. **Details are found cheaply and read later.** `details::details` decides
    from the tree and a few `lstat`s what else an item is, since the panel
    asks on every pointer move; anything slower is a reading, which the
    window starts once it has been wanted for a moment, two at a time, and
    keeps until the next scan.

## Where changes belong

| change | where |
| --- | --- |
| measurement, filtering, parallelism | `crates/disktree-core/src/scan.rs` |
| what a node is, or a derived total | `crates/disktree-core/src/tree.rs` |
| tile geometry, nesting, the merged tail | `crates/disktree-core/src/treemap.rs` |
| a sunburst's or icicle's layout, in unit space | `crates/disktree-core/src/partition.rs` |
| how a level change moves | `crates/disktree-core/src/camera.rs` |
| anything that deletes, or refuses to | `crates/disktree-core/src/removal.rs` |
| what an item is beyond its size (a new kind is a `Detail` case, its reading, its card) | `crates/disktree-core/src/details.rs`, `crates/disktree-app/src/detail_cards.rs` |
| reading a git checkout, what removing it would lose | `crates/disktree-core/src/checkout.rs`, and `git.rs` for how git is run |
| free space and projections | `crates/disktree-core/src/space.rs` |
| what Windows lists, measures and compares differently | `crates/disktree-core/src/windows.rs` — with `macos.rs`, the only `unsafe` |
| what only macOS's Foundation reports: a volume's name, its purgeable space | `crates/disktree-core/src/macos.rs` — with `windows.rs`, the only `unsafe` |
| a key, a screen transition, a mark | `crates/disktree-app/src/state.rs` |
| spacing, type and size | `crates/disktree-app/src/ui.rs` — tokens only, no `px` in layout |
| the mosaic's painting or labels | `crates/disktree-app/src/treemap_view.rs` |
| where a chart's unit space lands on screen | `crates/disktree-app/src/charts.rs` |
| the menu bar, its checks and shortcuts | `crates/disktree-app/src/app_menu.rs` |
| a theme of disktree's own | `crates/disktree-app/src/themes.rs` |
| what is kept for the next launch | `crates/disktree-app/src/settings.rs` |
| layout of a screen | `crates/disktree-app/src/views.rs` |
| colours derived from the theme | `crates/disktree-app/src/palette.rs` |

## Verification expectations

* Size accounting, hardlinks, symlinks, hidden entries, depth limits, the
  removal guards and squarified layout are covered by `disktree-core` tests
  against real temporary trees.
* Git checkouts are covered by `checkout.rs` tests that build a bare origin,
  a clone and worktrees with git itself and read them: merges, squashes found
  by content and by patch, detached heads, locks, nested checkouts, ignored
  `.env` files, submodules kept out of the worktree list, and a checkout's
  own fsmonitor, signature program and filter drivers never running. The
  merge base comes from `rev-list --boundary` and the squash search has a
  time budget because a monorepo with a hundred thousand remote refs makes
  the obvious commands take seconds.
* The screens are covered by window-harness tests that draw frames and press
  keys, including one that marks a directory, confirms the removal and checks
  the files are gone while unmarked neighbours are untouched.
* Rendering was verified by those tests and by running the app against a real
  home directory; it has not been eyeballed in every theme and font size.
