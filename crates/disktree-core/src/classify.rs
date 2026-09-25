//! What a directory *is*, and whether its space can be had back.
//!
//! Colour in the treemap means a kind of data, and a hatch means reclaimable
//! space, so the two questions a cleanup tool exists to answer — "what is it"
//! and "can I delete it" — can be read off a tile at a glance.
//!
//! Both come from names. A short lookup of well-known directory names covers
//! most of a home directory; anything unmatched takes its parent's kind, and a
//! top-level directory with an unknown name takes the kind of its largest
//! recognisable child (`~/world` is mostly `.git`, so it is git). Reclaimable
//! space is the same idea, plus a sibling check where a name alone is too
//! common to trust: `target` is only a build directory beside a `Cargo.toml`.

use crate::tree::Node;

/// A kind of data, for colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Category {
    /// Source code and checkouts.
    Code,
    /// Space agents write into: worktrees, sandboxes, experiments.
    AgentScratch,
    /// Compilers, package managers and their installs.
    Toolchain,
    /// Folders a sync client owns.
    Synced,
    /// Version-control object stores.
    Git,
    /// Pictures, music, video, games and models.
    Media,
    /// Application bundles and the folders holding them.
    Apps,
    /// Documents, downloads and the desktop.
    Documents,
    /// Caches and other regenerable state.
    Cache,
    /// Nothing recognisable.
    #[default]
    Other,
}

impl Category {
    /// The categories the legend lists, in its order. Apps are listed only
    /// on macOS, the one system that installs them as bundles.
    pub const LEGEND: &[Self] = &[
        Self::Code,
        Self::AgentScratch,
        Self::Toolchain,
        Self::Synced,
        Self::Git,
        Self::Media,
        #[cfg(target_os = "macos")]
        Self::Apps,
        Self::Documents,
        Self::Cache,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::AgentScratch => "Agent scratch",
            Self::Toolchain => "Toolchains",
            Self::Synced => "Synced",
            Self::Git => "Git",
            Self::Media => "Media",
            Self::Apps => "Apps",
            Self::Documents => "Documents",
            Self::Cache => "Cache",
            Self::Other => "Other",
        }
    }
}

/// Why a directory's space can be had back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reclaim {
    /// A cache: whatever wrote it will write it again.
    Regenerable,
    /// A sync client's old versions of files.
    SyncHistory,
    /// A package manager's content store.
    PackageStore,
    /// Compiler or bundler output beside its sources.
    BuildOutput,
    /// Installed dependencies beside their manifest.
    Reinstallable,
    /// Container or sandbox image layers.
    SandboxLayers,
    /// Sandbox or VM snapshots.
    Snapshots,
    /// Already deleted, still on disk.
    Trash,
    /// Scratch space meant to be thrown away.
    Temporary,
}

impl Reclaim {
    /// The reason, as the "Worth a look" list says it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Regenerable => "regenerable",
            Self::SyncHistory => "sync history",
            Self::PackageStore => "package store",
            Self::BuildOutput => "build output",
            Self::Reinstallable => "reinstallable",
            Self::SandboxLayers => "sandbox layers",
            Self::Snapshots => "snapshots",
            Self::Trash => "trash",
            Self::Temporary => "temporary",
        }
    }
}

/// The kind a directory name announces, if any. `parent`, the name of the
/// directory holding it, tells apart the few names that mean two things.
pub fn category_of_name(name: &str, parent: Option<&str>) -> Option<Category> {
    let lower = name.to_ascii_lowercase();
    // Windows: a work or school account's folder carries the organisation,
    // `OneDrive - Contoso`, and Dropbox's does the same, `Dropbox (Contoso)`.
    if lower.starts_with("onedrive - ") || lower.starts_with("dropbox (") {
        return Some(Category::Synced);
    }
    // macOS: application bundles, and the bundles Photos, Music, TV, Final
    // Cut Pro, iMovie, Logic Pro and GarageBand keep libraries and projects in.
    match lower.rsplit_once('.').map(|(_, extension)| extension) {
        Some("app") => return Some(Category::Apps),
        Some(
            "photoslibrary" | "musiclibrary" | "tvlibrary" | "fcpbundle"
            | "imovielibrary" | "logicx" | "band",
        ) => return Some(Category::Media),
        _ => {}
    }
    let under = |wanted: &str| {
        parent.is_some_and(|parent| parent.eq_ignore_ascii_case(wanted))
    };
    let category = match lower.as_str() {
        // macOS: ~/Library/Developer is Xcode's own state, and ~/Developer
        // is where Xcode puts projects. Inside an app bundle,
        // `Contents/Developer` is part of the app.
        "developer" if under("library") => Category::Toolchain,
        "developer" if under("contents") => return None,
        "src" | "code" | "projects" | "repos" | "dev" | "work"
        | "workspace" | "workspaces" | "github.com" | "gitlab.com"
        | "sites" | "development" | "developer" | "xcode projects" => {
            Category::Code
        }
        ".codex" | ".claude" | ".herdr" | ".pi" | ".cursor" | ".aider"
        | ".gemini" | ".continue" | ".windsurf" | ".microsandbox" | ".omp"
        | ".agents" | ".openai" | "tries" | "worktrees" | "experiments"
        | "scratch" | "playground" => Category::AgentScratch,
        ".cargo" | ".rustup" | ".local" | ".npm" | ".pnpm-store" | "pnpm"
        | ".bun" | ".deno" | "go" | ".gradle" | ".m2" | ".platformio"
        | "mise" | ".mise" | ".pyenv" | ".nvm" | ".gem" | "gem" | ".rbenv"
        | ".espressif" | ".arduino15" | ".config" | ".vscode" | ".zig"
        | ".rye" | ".conda" | "anaconda3" | "miniconda3" | ".opam"
        | ".ghcup" | ".stack" | ".julia" | ".dotnet" | ".android"
        | ".sdkman" | ".volta" | ".yarn" | ".java" | ".nuget" | ".swiftpm"
        | ".docker" | ".orbstack" | ".colima" | ".lima"
        // macOS: Homebrew's prefix, Android Studio's SDK in ~/Library,
        // Docker Desktop's container and the Command Line Tools.
        | "homebrew" | ".homebrew" | "android" | "com.docker.docker"
        | "commandlinetools"
        // macOS: Xcode's and the simulators' state in ~/Library/Developer.
        | "xcode" | "coresimulator" => Category::Toolchain,
        "sync" | "dropbox" | "nextcloud" | "google drive" | "onedrive"
        | "pclouddrive" | "mega" | ".stversions"
        // macOS: iCloud Drive, and the File Provider clients (Dropbox,
        // Google Drive, OneDrive) since macOS 12.
        | "mobile documents" | "icloud drive" | "cloudstorage"
        // macOS: iPhone and iPad backups, in ~/Library/Application Support.
        | "mobilesync"
        // Windows: iCloud for Windows.
        | "iclouddrive" => Category::Synced,
        ".git" => Category::Git,
        "pictures" | "photos" | "music" | "videos" | "movies" | "steam"
        | "models" | ".ollama" | ".lmstudio" | "games" | "wineprefix" => {
            Category::Media
        }
        // Linux has `applications` too, of launcher entries, which the
        // legend does not name.
        "applications" if cfg!(target_os = "macos") => Category::Apps,
        "documents" | "desktop" | "downloads" | "books" | "notes"
        | "obsidian" | "public" | "templates" => Category::Documents,
        ".cache" | "cache" | "caches" | ".ccache" | ".sccache" | "_cacache"
        | "__pycache__" | "node_modules" | "trash" | ".trash" | "tmp"
        | ".tmp" | "deriveddata" | "ios devicesupport"
        | "watchos devicesupport" | "tvos devicesupport"
        | "xros devicesupport"
        // Windows: see `reclaim_of`.
        | "temp" | "$recycle.bin" | "npm-cache" | "v3-cache" | "inetcache"
        | "d3dscache" | "dxcache" | "glcache" | "crashdumps" => Category::Cache,
        _ => return None,
    };
    Some(category)
}

/// Whether a directory's space can be had back, judged from its name, the
/// kind of the directory holding it, and its siblings' names.
pub fn reclaim_of(
    name: &str,
    parent: Category,
    has_sibling: impl Fn(&str) -> bool,
) -> Option<Reclaim> {
    let lower = name.to_ascii_lowercase();
    let reclaim = match lower.as_str() {
        ".cache" | "cache" | "caches" | ".ccache" | ".sccache" | "_cacache"
        // Windows' own caches in AppData\Local: npm's, NuGet's downloads,
        // the browser engine's, and compiled shaders, Direct3D's and the
        // graphics driver's, all rebuilt as they are needed.
        | "npm-cache" | "v3-cache" | "inetcache" | "d3dscache" | "dxcache"
        | "glcache"
        // Symbols Xcode copies off a device it meets, and copies again the
        // next time that device is plugged in.
        | "ios devicesupport" | "watchos devicesupport"
        | "tvos devicesupport" | "xros devicesupport" => Reclaim::Regenerable,
        ".stversions" => Reclaim::SyncHistory,
        ".pnpm-store" | "pnpm" => Reclaim::PackageStore,
        "__pycache__" | ".pytest_cache" | ".mypy_cache" | ".ruff_cache"
        | ".next" | ".turbo" | ".parcel-cache"
        // Xcode's build products and indexes, rebuilt on the next build.
        | "deriveddata" => Reclaim::BuildOutput,
        // ~/Library/Logs, told apart from a project's logs by its neighbour.
        "logs" if has_sibling("Application Support") => Reclaim::Temporary,
        // Too common to trust alone: only a build directory beside a manifest.
        "target" if has_sibling("Cargo.toml") => Reclaim::BuildOutput,
        ".build" if has_sibling("Package.swift") => Reclaim::BuildOutput,
        "node_modules" if has_sibling("package.json") => Reclaim::Reinstallable,
        "pods" if has_sibling("Podfile") => Reclaim::Reinstallable,
        // Layers and snapshots are only disposable inside sandbox state.
        "layers" if parent == Category::AgentScratch => Reclaim::SandboxLayers,
        "snapshots" if parent == Category::AgentScratch => Reclaim::Snapshots,
        "trash" | ".trash" | "$recycle.bin" => Reclaim::Trash,
        // `Temp` is where Windows puts temporary files, in AppData\Local;
        // `CrashDumps` beside it holds dumps of programs that crashed.
        "tmp" | ".tmp" | "temp" | "crashdumps" => Reclaim::Temporary,
        _ => return None,
    };
    Some(reclaim)
}

/// Assign a category and a reclaim reason to every node beneath `root`.
///
/// Top-down: a node's own name wins, otherwise it inherits. Reclaimable
/// space is inherited too, so everything under a cache is hatched.
pub fn classify(root: &mut Node) {
    root.category = Category::Other;
    root.reclaim = None;
    let children = std::mem::take(&mut root.children);
    let names: Vec<Box<str>> =
        children.iter().map(|child| child.name.clone()).collect();
    root.children = children;
    for index in 0..root.children.len() {
        let has_sibling =
            |wanted: &str| names.iter().any(|name| &**name == wanted);
        let child = &mut root.children[index];
        // A top-level directory with an unknown name takes the kind of its
        // largest recognisable child: `~/world` is mostly `.git`.
        let category = category_of_name(&child.name, Some(&root.name))
            .or_else(|| is_git_store(child).then_some(Category::Git))
            .or_else(|| dominant_child_category(child))
            .unwrap_or(Category::Other);
        let reclaim = child
            .is_dir()
            .then(|| reclaim_of(&child.name, Category::Other, has_sibling))
            .flatten();
        classify_below(child, category, reclaim);
    }
}

fn classify_below(
    node: &mut Node,
    category: Category,
    reclaim: Option<Reclaim>,
) {
    node.category = category;
    node.reclaim = reclaim;
    if node.children.is_empty() {
        return;
    }
    let names: Vec<Box<str>> = node
        .children
        .iter()
        .map(|child| child.name.clone())
        .collect();
    for child in &mut node.children {
        let has_sibling =
            |wanted: &str| names.iter().any(|name| &**name == wanted);
        let child_category = if child.is_dir() {
            category_of_name(&child.name, Some(&node.name))
                .or_else(|| is_git_store(child).then_some(Category::Git))
                .unwrap_or(category)
        } else {
            category
        };
        let child_reclaim = reclaim.or_else(|| {
            child
                .is_dir()
                .then(|| reclaim_of(&child.name, category, has_sibling))
                .flatten()
        });
        classify_below(child, child_category, child_reclaim);
    }
}

/// The kind of an unknown directory, from what fills it: the first
/// recognisable name down the largest children, a few levels deep.
fn dominant_child_category(node: &Node) -> Option<Category> {
    let mut node = node;
    for _ in 0..3 {
        if let Some(category) = node
            .children
            .iter()
            .filter(|child| child.is_dir())
            .find_map(|child| {
                category_of_name(&child.name, Some(&node.name))
                    .or_else(|| is_git_store(child).then_some(Category::Git))
            })
        {
            return Some(category);
        }
        node = node.children.iter().find(|child| child.is_dir())?;
    }
    None
}

/// A git object store by its shape, whatever it is called: a bare
/// repository, or a `.git` directory, has `objects`, `refs` and `HEAD`.
pub fn is_git_store(node: &Node) -> bool {
    let has =
        |wanted: &str| node.children.iter().any(|child| &*child.name == wanted);
    node.is_dir() && has("objects") && has("refs") && has("HEAD")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Metric, NodeKind, aggregate};

    fn file(name: &str, bytes: u64) -> Node {
        Node::entry(name, NodeKind::File, bytes)
    }

    fn dir(name: &str, children: Vec<Node>) -> Node {
        let mut node = Node::directory(name);
        node.children = children;
        node
    }

    fn home() -> Node {
        let mut home = dir(
            "tobi",
            vec![
                dir(
                    "src",
                    vec![dir(
                        "tries",
                        vec![dir("2026-09-01", vec![file("a", 1)])],
                    )],
                ),
                dir(
                    ".cache",
                    vec![dir("kache", vec![dir("store", vec![file("b", 1)])])],
                ),
                dir(
                    "world",
                    vec![
                        dir(".git", vec![dir("objects", vec![file("c", 9)])]),
                        file("README", 1),
                    ],
                ),
                dir(
                    "rust-thing",
                    vec![
                        file("Cargo.toml", 1),
                        dir("target", vec![file("d", 5)]),
                    ],
                ),
                dir("js-thing", vec![dir("target", vec![file("e", 5)])]),
                dir(
                    ".microsandbox",
                    vec![
                        dir("cache", vec![dir("layers", vec![file("f", 1)])]),
                        dir("snapshots", vec![file("g", 1)]),
                    ],
                ),
                dir("Sync", vec![dir(".stversions", vec![file("h", 1)])]),
                dir("mystery", vec![file("i", 1)]),
                dir(
                    "monorepo",
                    vec![dir(
                        "git",
                        vec![
                            file("HEAD", 1),
                            dir("refs", vec![]),
                            dir("objects", vec![file("pack", 50)]),
                        ],
                    )],
                ),
                dir(
                    "Library",
                    vec![
                        dir(
                            "Caches",
                            vec![dir("com.apple.Safari", vec![file("j", 30)])],
                        ),
                        dir(
                            "Developer",
                            vec![dir(
                                "Xcode",
                                vec![dir(
                                    "DerivedData",
                                    vec![dir("App-abc", vec![file("k", 40)])],
                                )],
                            )],
                        ),
                        dir("Mobile Documents", vec![file("l", 2)]),
                    ],
                ),
                dir("Developer", vec![dir("app", vec![file("main.swift", 3)])]),
                dir(
                    "Applications",
                    vec![dir("Xcode.app", vec![file("m", 60)])],
                ),
                dir(
                    "Downloads",
                    vec![dir("Installer.app", vec![file("n", 9)])],
                ),
                dir(
                    "Pictures",
                    vec![dir(
                        "Photos Library.photoslibrary",
                        vec![file("o", 70)],
                    )],
                ),
                dir(
                    "swift-thing",
                    vec![
                        file("Package.swift", 1),
                        dir(".build", vec![file("p", 8)]),
                    ],
                ),
                dir(
                    "ios-thing",
                    vec![file("Podfile", 1), dir("Pods", vec![file("q", 6)])],
                ),
            ],
        );
        aggregate(&mut home, Metric::Bytes);
        classify(&mut home);
        home
    }

    fn named<'a>(node: &'a Node, path: &[&str]) -> &'a Node {
        let mut node = node;
        for part in path {
            node = node
                .children
                .iter()
                .find(|child| &*child.name == *part)
                .unwrap_or_else(|| panic!("no {part}"));
        }
        node
    }

    #[test]
    fn names_announce_their_kind_and_children_inherit_it() {
        let home = home();
        assert_eq!(named(&home, &["src"]).category, Category::Code);
        // `tries` is agent scratch even inside code: it is what agents write.
        assert_eq!(
            named(&home, &["src", "tries"]).category,
            Category::AgentScratch
        );
        assert_eq!(
            named(&home, &["src", "tries", "2026-09-01"]).category,
            Category::AgentScratch,
            "an unknown name inherits"
        );
        assert_eq!(
            named(&home, &["src", "tries", "2026-09-01", "a"]).category,
            Category::AgentScratch,
            "files inherit too"
        );
    }

    #[test]
    fn an_unknown_top_level_directory_takes_its_largest_known_child() {
        let home = home();
        assert_eq!(named(&home, &["world"]).category, Category::Git);
        assert_eq!(named(&home, &["mystery"]).category, Category::Other);
        // A bare repository is git by its shape, whatever it is called.
        assert_eq!(named(&home, &["monorepo"]).category, Category::Git);
        assert_eq!(named(&home, &["monorepo", "git"]).category, Category::Git);
    }

    #[test]
    fn caches_are_reclaimable_all_the_way_down() {
        let home = home();
        assert_eq!(
            named(&home, &[".cache"]).reclaim,
            Some(Reclaim::Regenerable)
        );
        assert_eq!(
            named(&home, &[".cache", "kache", "store"]).reclaim,
            Some(Reclaim::Regenerable)
        );
        assert_eq!(
            named(&home, &["Sync", ".stversions"]).reclaim,
            Some(Reclaim::SyncHistory)
        );
        assert_eq!(named(&home, &["src"]).reclaim, None);
        assert_eq!(
            named(&home, &["Library", "Caches", "com.apple.Safari"]).reclaim,
            Some(Reclaim::Regenerable)
        );
    }

    #[test]
    fn target_is_build_output_only_beside_a_cargo_manifest() {
        let home = home();
        assert_eq!(
            named(&home, &["rust-thing", "target"]).reclaim,
            Some(Reclaim::BuildOutput)
        );
        assert_eq!(named(&home, &["js-thing", "target"]).reclaim, None);
    }

    #[test]
    fn swift_builds_and_pods_are_reclaimable_only_beside_their_manifest() {
        let home = home();
        assert_eq!(
            named(&home, &["swift-thing", ".build"]).reclaim,
            Some(Reclaim::BuildOutput)
        );
        assert_eq!(
            named(&home, &["ios-thing", "Pods"]).reclaim,
            Some(Reclaim::Reinstallable)
        );
        let none = |_: &str| false;
        assert_eq!(reclaim_of(".build", Category::Code, none), None);
        assert_eq!(reclaim_of("Pods", Category::Code, none), None);
    }

    #[test]
    fn sandbox_layers_and_snapshots_are_reclaimable_only_in_sandbox_state() {
        let home = home();
        // .microsandbox/cache is already a regenerable cache, so its layers
        // inherit that; the snapshots are judged on their own.
        assert!(
            named(&home, &[".microsandbox", "cache", "layers"])
                .reclaim
                .is_some()
        );
        assert_eq!(
            named(&home, &[".microsandbox", "snapshots"]).reclaim,
            Some(Reclaim::Snapshots)
        );
        assert_eq!(
            reclaim_of("snapshots", Category::Documents, |_| false),
            None
        );
    }

    #[test]
    fn macos_developer_leftovers_are_reclaimable_and_its_risks_are_not() {
        let none = |_: &str| false;
        assert_eq!(
            reclaim_of("DerivedData", Category::Toolchain, none),
            Some(Reclaim::BuildOutput)
        );
        for name in [
            "iOS DeviceSupport",
            "watchOS DeviceSupport",
            "tvOS DeviceSupport",
            "xrOS DeviceSupport",
        ] {
            assert_eq!(
                reclaim_of(name, Category::Toolchain, none),
                Some(Reclaim::Regenerable),
                "{name}"
            );
            assert_eq!(
                category_of_name(name, Some("Xcode")),
                Some(Category::Cache),
                "{name}"
            );
        }
        let library = |name: &str| name == "Application Support";
        assert_eq!(
            reclaim_of("Logs", Category::Other, library),
            Some(Reclaim::Temporary)
        );
        assert_eq!(reclaim_of("logs", Category::Code, none), None);
        // Big, but not safe to offer: Archives hold the symbols crash
        // reports need, and Backup is an iPhone's only backup.
        assert_eq!(reclaim_of("Archives", Category::Toolchain, none), None);
        assert_eq!(reclaim_of("Backup", Category::Other, none), None);
        assert_eq!(
            category_of_name("Mobile Documents", Some("Library")),
            Some(Category::Synced)
        );
        assert_eq!(
            category_of_name("Xcode", Some("Developer")),
            Some(Category::Toolchain)
        );
    }

    #[test]
    fn macos_folders_are_recognised() {
        let home = home();
        // ~/Developer is where Xcode puts projects; ~/Library/Developer is
        // Xcode's own state, told apart only by the parent.
        assert_eq!(named(&home, &["Developer"]).category, Category::Code);
        assert_eq!(
            named(&home, &["Library", "Developer"]).category,
            Category::Toolchain
        );
        assert_eq!(
            named(&home, &["Library", "Developer", "Xcode", "DerivedData"])
                .reclaim,
            Some(Reclaim::BuildOutput)
        );
        assert_eq!(
            named(&home, &["Library", "Mobile Documents"]).category,
            Category::Synced
        );
        if cfg!(target_os = "macos") {
            assert_eq!(
                named(&home, &["Applications"]).category,
                Category::Apps
            );
        }
        assert_eq!(
            named(&home, &["Applications", "Xcode.app"]).category,
            Category::Apps
        );
        assert_eq!(
            category_of_name("Developer", Some("Contents")),
            None,
            "Xcode.app/Contents/Developer is part of the app"
        );
        // A bundle is an app wherever it sits, not only in Applications.
        assert_eq!(
            named(&home, &["Downloads", "Installer.app"]).category,
            Category::Apps
        );
        assert_eq!(
            named(&home, &["Pictures", "Photos Library.photoslibrary"])
                .category,
            Category::Media
        );
        assert_eq!(
            category_of_name("developer", Some("LIBRARY")),
            Some(Category::Toolchain)
        );
        assert_eq!(category_of_name("Developer", None), Some(Category::Code));
    }

    #[test]
    fn app_and_media_bundles_are_known_by_their_suffix() {
        for name in ["Safari.app", "Old Thing.APP"] {
            assert_eq!(
                category_of_name(name, None),
                Some(Category::Apps),
                "{name}"
            );
        }
        for name in [
            "Photos Library.photoslibrary",
            "Music Library.musiclibrary",
            "TV Library.tvlibrary",
            "Untitled.fcpbundle",
            "iMovie Library.imovielibrary",
            "Song.logicx",
            "Demo.band",
            "Shouting.PHOTOSLIBRARY",
        ] {
            assert_eq!(
                category_of_name(name, None),
                Some(Category::Media),
                "{name}"
            );
        }
        assert_eq!(category_of_name("app", None), None);
    }

    #[test]
    fn the_macs_toolchains_and_sync_folders_are_recognised() {
        for (name, category) in [
            ("Xcode Projects", Category::Code),
            (".swiftpm", Category::Toolchain),
            ("Homebrew", Category::Toolchain),
            (".homebrew", Category::Toolchain),
            ("Android", Category::Toolchain),
            (".docker", Category::Toolchain),
            ("com.docker.docker", Category::Toolchain),
            (".orbstack", Category::Toolchain),
            (".colima", Category::Toolchain),
            (".lima", Category::Toolchain),
            ("CommandLineTools", Category::Toolchain),
            ("CoreSimulator", Category::Toolchain),
            ("iCloud Drive", Category::Synced),
            ("MobileSync", Category::Synced),
        ] {
            assert_eq!(category_of_name(name, None), Some(category), "{name}");
        }
    }

    #[test]
    fn windows_caches_and_sync_folders_are_recognized() {
        let none = |_: &str| false;
        for (name, reclaim) in [
            ("Temp", Reclaim::Temporary),
            ("CrashDumps", Reclaim::Temporary),
            ("npm-cache", Reclaim::Regenerable),
            ("v3-cache", Reclaim::Regenerable),
            ("INetCache", Reclaim::Regenerable),
            ("D3DSCache", Reclaim::Regenerable),
            ("DXCache", Reclaim::Regenerable),
            ("$Recycle.Bin", Reclaim::Trash),
        ] {
            assert_eq!(
                reclaim_of(name, Category::Other, none),
                Some(reclaim),
                "{name}"
            );
            assert_eq!(
                category_of_name(name, None),
                Some(Category::Cache),
                "{name}"
            );
        }
        for name in ["OneDrive - Contoso", "Dropbox (Contoso)", "iCloudDrive"] {
            assert_eq!(
                category_of_name(name, None),
                Some(Category::Synced),
                "{name}"
            );
        }
        assert_eq!(category_of_name(".nuget", None), Some(Category::Toolchain));
        assert_eq!(category_of_name("OneDriveSetup", None), None);
    }

    #[test]
    fn the_legend_lists_every_named_category_once() {
        let mut seen = std::collections::HashSet::new();
        for &category in Category::LEGEND {
            assert!(seen.insert(category));
            assert_ne!(category, Category::Other);
            assert!(!category.label().is_empty());
        }
        assert_eq!(
            Category::LEGEND.contains(&Category::Apps),
            cfg!(target_os = "macos"),
            "apps are listed only where they are bundles"
        );
    }
}
