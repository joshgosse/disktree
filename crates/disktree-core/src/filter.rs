//! Typeahead filtering: which parts of a tree match a name.
//!
//! A node whose name contains the needle (ignoring ASCII case) matches, and
//! is kept whole: everything under a matching directory goes with it. Its
//! ancestors are kept only partly, and sized by what matched beneath them,
//! so a filtered treemap shows exactly the matches, at their true relative
//! sizes, in the places they live.

use std::sync::atomic::{AtomicBool, Ordering};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashMap;

use crate::tree::{Metric, Node};

/// Smaller searches finish on one thread before pieces could be handed out.
const SPLIT_FROM: u64 = 200_000;
/// Smaller pieces cost more to hand out than to search.
const SMALLEST_PIECE: u64 = 20_000;
/// Several pieces a thread, so one that draws a slow piece does not hold up
/// the rest.
const PIECES_PER_THREAD: u64 = 8;

/// How a node takes part in a filtered view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// It matches: drawn as usual, with everything beneath it.
    Whole,
    /// It holds matches: drawn with only those, at their size.
    Partial { bytes: u64, files: u64 },
}

/// The outcome of filtering a subtree by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matches {
    /// The needle, lowercased.
    pub needle: String,
    /// Absolute crumbs of the subtree that was searched.
    pub base: Vec<usize>,
    /// Keyed by absolute crumbs. Only matches and their ancestors appear;
    /// a match's own descendants are implied.
    pub keep: FxHashMap<Vec<usize>, Keep>,
    /// Topmost matches: a match inside a match is not counted again.
    pub count: usize,
    pub bytes: u64,
    pub files: u64,
}

impl Matches {
    /// How the node at `crumbs` takes part: `None` when it is filtered out.
    /// Anything outside the searched subtree, and anything beneath a match,
    /// is kept whole.
    pub fn keep(&self, crumbs: &[usize]) -> Option<Keep> {
        if !crumbs.starts_with(&self.base) {
            return Some(Keep::Whole);
        }
        for length in self.base.len()..=crumbs.len() {
            match self.keep.get(&crumbs[..length]) {
                Some(Keep::Whole) => return Some(Keep::Whole),
                Some(partial) if length == crumbs.len() => {
                    return Some(*partial);
                }
                // Only the base may be absent: it holds the matches but is
                // not recorded, being where the search started.
                None if length > self.base.len() => return None,
                _ => {}
            }
        }
        // The base itself holds the matches.
        Some(Keep::Partial {
            bytes: self.bytes,
            files: self.files,
        })
    }

    /// The value a kept node is laid out by.
    pub const fn value(keep: Keep, node: &Node, metric: Metric) -> u64 {
        match (keep, metric) {
            (Keep::Whole, _) => node.value(metric),
            (Keep::Partial { bytes, .. }, Metric::Bytes) => bytes,
            (Keep::Partial { files, .. }, Metric::Files) => files,
        }
    }
}

/// Search `node`, found at absolute `base`, for names containing `needle`.
/// `None` for an empty needle: nothing is filtered.
///
/// `None` too once `cancel` is set: it is read as the search goes, so a
/// superseded search stops early. A large tree is searched in pieces across
/// rayon's threads.
pub fn filter(
    node: &Node,
    base: &[usize],
    needle: &str,
    cancel: &AtomicBool,
) -> Option<Matches> {
    let entries = entries(node);
    let pieces = rayon::current_num_threads() as u64 * PIECES_PER_THREAD;
    let limit =
        (entries >= SPLIT_FROM).then(|| (entries / pieces).max(SMALLEST_PIECE));
    filter_splitting_above(node, base, needle, cancel, limit)
}

/// With a `limit`, the directories the search descends into are cut into
/// pieces of at most that many entries, searched in parallel; without one,
/// the search runs on the caller's thread.
fn filter_splitting_above(
    node: &Node,
    base: &[usize],
    needle: &str,
    cancel: &AtomicBool,
    limit: Option<u64>,
) -> Option<Matches> {
    let needle = needle.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    let matches = search(node, base, needle, cancel, limit);
    (!cancel.load(Ordering::Relaxed)).then_some(matches)
}

fn search(
    node: &Node,
    base: &[usize],
    needle: String,
    cancel: &AtomicBool,
    limit: Option<u64>,
) -> Matches {
    let mut matches = Matches {
        needle,
        base: base.to_vec(),
        ..Matches::default()
    };
    let mut crumbs = base.to_vec();
    (matches.bytes, matches.files) = match limit {
        Some(limit) => {
            search_in_pieces(node, &mut crumbs, limit, cancel, &mut matches)
        }
        None => visit(node, &mut crumbs, cancel, &mut matches),
    };
    matches
}

const fn entries(node: &Node) -> u64 {
    node.files + node.dirs
}

/// One directory's children: a match is kept whole, and a directory that did
/// not match is kept partly by what `below` found inside it. Returns the
/// bytes and files that matched.
fn level(
    node: &Node,
    crumbs: &mut Vec<usize>,
    matches: &mut Matches,
    mut below: impl FnMut(&Node, &mut Vec<usize>, &mut Matches) -> (u64, u64),
) -> (u64, u64) {
    let mut total = (0, 0);
    for (index, child) in node.children.iter().enumerate() {
        crumbs.push(index);
        if contains_ignoring_case(&child.name, &matches.needle) {
            matches.keep.insert(crumbs.clone(), Keep::Whole);
            matches.count += 1;
            total.0 += child.bytes;
            total.1 += child.files;
        } else if !child.children.is_empty() {
            let (bytes, files) = below(child, crumbs, matches);
            if bytes > 0 || files > 0 {
                matches
                    .keep
                    .insert(crumbs.clone(), Keep::Partial { bytes, files });
                total.0 += bytes;
                total.1 += files;
            }
        }
        crumbs.pop();
    }
    total
}

fn visit(
    node: &Node,
    crumbs: &mut Vec<usize>,
    cancel: &AtomicBool,
    matches: &mut Matches,
) -> (u64, u64) {
    if cancel.load(Ordering::Relaxed) {
        return (0, 0);
    }
    level(node, crumbs, matches, |child, crumbs, matches| {
        visit(child, crumbs, cancel, matches)
    })
}

/// A directory the search descends into, searched on its own.
struct Piece<'a> {
    crumbs: Vec<usize>,
    node: &'a Node,
}

/// Each piece is searched as a subtree of its own, where it lies, and
/// `assemble` then walks what is left, taking a piece's result wherever the
/// walk meets it.
fn search_in_pieces(
    node: &Node,
    crumbs: &mut Vec<usize>,
    limit: u64,
    cancel: &AtomicBool,
    matches: &mut Matches,
) -> (u64, u64) {
    let mut pieces = Vec::new();
    split(node, crumbs, &matches.needle, limit, &mut pieces);
    let found: Vec<Matches> = pieces
        .par_iter()
        .map(|piece| {
            let needle = matches.needle.clone();
            search(piece.node, &piece.crumbs, needle, cancel, None)
        })
        .collect();
    matches
        .keep
        .reserve(found.iter().map(|piece| piece.keep.len()).sum());
    assemble(node, crumbs, limit, &mut found.into_iter(), matches)
}

/// The directories `level` would descend into, cut into pieces of at most
/// `limit` entries, in the order `assemble` meets them.
fn split<'a>(
    node: &'a Node,
    crumbs: &mut Vec<usize>,
    needle: &str,
    limit: u64,
    pieces: &mut Vec<Piece<'a>>,
) {
    for (index, child) in node.children.iter().enumerate() {
        if child.children.is_empty()
            || contains_ignoring_case(&child.name, needle)
        {
            continue;
        }
        crumbs.push(index);
        if entries(child) > limit {
            split(child, crumbs, needle, limit, pieces);
        } else {
            pieces.push(Piece {
                crumbs: crumbs.clone(),
                node: child,
            });
        }
        crumbs.pop();
    }
}

fn assemble(
    node: &Node,
    crumbs: &mut Vec<usize>,
    limit: u64,
    found: &mut impl Iterator<Item = Matches>,
    matches: &mut Matches,
) -> (u64, u64) {
    level(node, crumbs, matches, |child, crumbs, matches| {
        if entries(child) > limit {
            return assemble(child, crumbs, limit, found, matches);
        }
        let piece = found.next().expect("split cut a piece here");
        matches.keep.extend(piece.keep);
        matches.count += piece.count;
        (piece.bytes, piece.files)
    })
}

/// Substring search ignoring ASCII case, without allocating: a filter runs
/// over every name in view on every keystroke.
fn contains_ignoring_case(haystack: &str, lower_needle: &str) -> bool {
    let (hay, needle) = (haystack.as_bytes(), lower_needle.as_bytes());
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.to_ascii_lowercase() == *right)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NodeKind, aggregate};

    const fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn file(name: &str, bytes: u64) -> Node {
        Node::entry(name, NodeKind::File, bytes)
    }

    fn dir(name: &str, children: Vec<Node>) -> Node {
        let mut node = Node::directory(name);
        node.children = children;
        node
    }

    fn tree() -> Node {
        let mut root = dir(
            "root",
            vec![
                dir(
                    "src",
                    vec![
                        dir("App", vec![file("main.rs", 10)]),
                        file("notes", 5),
                    ],
                ),
                dir(
                    "apps",
                    vec![file("x", 100), dir("apple", vec![file("y", 7)])],
                ),
                file("readme", 1),
            ],
        );
        aggregate(&mut root, Metric::Bytes);
        root
    }

    fn crumbs_of(root: &Node, names: &[&str]) -> Vec<usize> {
        let mut node = root;
        names
            .iter()
            .map(|name| {
                let index = node
                    .children
                    .iter()
                    .position(|child| &*child.name == *name)
                    .expect("present");
                node = &node.children[index];
                index
            })
            .collect()
    }

    #[test]
    fn matches_are_kept_whole_and_their_ancestors_by_what_matched() {
        let root = tree();
        let found = filter(&root, &[], "APP", &never()).expect("a needle");
        assert_eq!(found.count, 2, "src/App and apps; apple is inside apps");
        assert_eq!(found.bytes, 10 + 107);
        assert_eq!(found.keep(&crumbs_of(&root, &["apps"])), Some(Keep::Whole));
        assert_eq!(
            found.keep(&crumbs_of(&root, &["apps", "x"])),
            Some(Keep::Whole),
            "inside a match"
        );
        assert_eq!(
            found.keep(&crumbs_of(&root, &["src"])),
            Some(Keep::Partial {
                bytes: 10,
                files: 1
            })
        );
        assert_eq!(found.keep(&crumbs_of(&root, &["src", "notes"])), None);
        assert_eq!(found.keep(&crumbs_of(&root, &["readme"])), None);
    }

    #[test]
    fn a_search_below_the_root_leaves_the_rest_alone() {
        let root = tree();
        let src = crumbs_of(&root, &["src"]);
        let src_node = root.resolve(&src).expect("src");
        let found = filter(src_node, &src, "main", &never()).expect("needle");
        assert_eq!(found.keep(&crumbs_of(&root, &["apps"])), Some(Keep::Whole));
        assert_eq!(found.keep(&crumbs_of(&root, &["src", "notes"])), None);
        assert_eq!(
            found.keep(&src),
            Some(Keep::Partial {
                bytes: 10,
                files: 1
            })
        );
    }

    #[test]
    fn an_empty_needle_filters_nothing() {
        assert!(filter(&tree(), &[], "  ", &never()).is_none());
        assert!(
            filter_splitting_above(&tree(), &[], "  ", &never(), Some(1))
                .is_none()
        );
        let found = filter(&tree(), &[], "zzz", &never()).expect("needle");
        assert_eq!(found.count, 0);
        assert!(found.keep.is_empty());
    }

    #[test]
    fn a_cancelled_search_has_no_result() {
        let cancelled = AtomicBool::new(true);
        assert!(filter(&tree(), &[], "app", &cancelled).is_none());
        assert!(
            filter_splitting_above(&tree(), &[], "app", &cancelled, Some(1))
                .is_none(),
            "in pieces too"
        );
    }

    /// A linear congruential generator: the same trees on every run.
    struct Lcg(u64);

    impl Lcg {
        const fn below(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % bound
        }
    }

    /// Names from a three-letter alphabet, so short needles match at every
    /// depth, inside matches and beside them, directories as well as files.
    fn random_tree(random: &mut Lcg, name: &str, depth: u32) -> Node {
        let width = 4 + random.below(if depth < 2 { 14 } else { 8 });
        let mut children = Vec::new();
        for index in 0..width {
            let mut child = String::new();
            for _ in 0..=random.below(4) {
                child.push(char::from(b"abc"[random.below(3) as usize]));
            }
            child.push_str(&index.to_string());
            children.push(if depth < 4 && random.below(2) == 0 {
                random_tree(random, &child, depth + 1)
            } else {
                file(&child, random.below(1000))
            });
        }
        dir(name, children)
    }

    #[test]
    fn a_search_in_pieces_finds_exactly_what_one_thread_finds() {
        for seed in 1..=4 {
            let mut root = random_tree(&mut Lcg(seed), "root", 0);
            aggregate(&mut root, Metric::Bytes);
            assert!(entries(&root) > 1_000, "enough to cut into many pieces");
            let whole = filter(&root, &[], "a", &never()).expect("needle");
            assert!(
                whole.keep.iter().any(|(crumbs, keep)| {
                    *keep == Keep::Whole
                        && root.resolve(crumbs).is_some_and(|node| {
                            node.children
                                .iter()
                                .any(|child| child.name.contains('a'))
                        })
                }),
                "a directory matches by name, with a match inside it"
            );
            let sub = root
                .children
                .iter()
                .position(|child| !child.children.is_empty())
                .expect("a directory");
            for (node, base) in
                [(&root, vec![]), (&root.children[sub], vec![sub])]
            {
                for needle in ["a", "ab", "bca", "cab", "zz"] {
                    let one = filter_splitting_above(
                        node,
                        &base,
                        needle,
                        &never(),
                        None,
                    );
                    for limit in [1, 7, 60, 500] {
                        let pieces = filter_splitting_above(
                            node,
                            &base,
                            needle,
                            &never(),
                            Some(limit),
                        );
                        assert_eq!(
                            pieces, one,
                            "seed {seed}, needle {needle}, base {base:?}, \
                             pieces of at most {limit}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn case_is_ignored_without_allocating() {
        assert!(contains_ignoring_case("Cargo.TOML", "toml"));
        assert!(!contains_ignoring_case("ab", "abc"));
        assert!(contains_ignoring_case("x", ""));
    }
}
