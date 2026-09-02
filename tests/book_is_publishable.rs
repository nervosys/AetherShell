//! The documentation book must be something a reader can actually reach.
//!
//! `docs/book` holds 47 chapters and has existed since the project started. It
//! had never been built, by CI or by anything else, and three separate reasons
//! for that had accumulated undetected:
//!
//!   * `book.toml` did not parse. It still carried `multilingual`,
//!     `use-hierarchical-outline` and `git-repository-icon = "fa-github"`, all
//!     removed by mdBook, plus an `additional-js = ["highlight.js"]` naming a
//!     file that does not exist and an `additional-css` path resolved relative
//!     to the book root while the file sat in `src/`.
//!   * Two SUMMARY entries pointed at `./changelog.md` and `./faq.md` while the
//!     real chapters sat in `appendix/`, and `create-missing = true` meant
//!     mdBook silently invented empty files for both instead of complaining.
//!   * The Pages workflow uploaded `website/` and nothing else, so the site
//!     served a single landing page and every "Documentation" link in the
//!     README and on that page pointed at `docs/TUI_GUIDE.md` — one markdown
//!     file, in the repository, standing in for the whole book.
//!
//! None of that is visible from a green test run, because nothing tested it.
//! This does. It does not need mdBook installed: the failures above are all
//! statements about files on disk.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn book_src() -> PathBuf {
    root().join("docs").join("book").join("src")
}

/// Every chapter path a SUMMARY links to, as written (`./ai/tools.md`).
fn summary_links(summary: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = summary.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == ']' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            let mut j = i + 2;
            let mut link = String::new();
            while j < bytes.len() && bytes[j] != ')' {
                link.push(bytes[j]);
                j += 1;
            }
            if link.ends_with(".md") {
                out.push(link);
            }
            i = j;
        }
        i += 1;
    }
    out
}

fn chapter_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&book_src(), &mut out);
    out
}

#[test]
fn every_summary_entry_resolves_to_a_real_chapter() {
    let summary = std::fs::read_to_string(book_src().join("SUMMARY.md")).expect("read SUMMARY.md");
    let links = summary_links(&summary);
    for link in &links {
        let rel = link.trim_start_matches("./");
        let path = book_src().join(rel);
        assert!(
            path.is_file(),
            "SUMMARY.md links to {link}, which does not exist. mdBook will not \
             build it (create-missing = false); fix the link or write the chapter."
        );
        let len = std::fs::metadata(&path).expect("stat chapter").len();
        assert!(
            len > 0,
            "{link} is empty. An empty chapter renders as a blank page, which is \
             what create-missing = true used to produce silently."
        );
    }
}

#[test]
fn no_chapter_is_stranded_outside_the_summary() {
    let summary = std::fs::read_to_string(book_src().join("SUMMARY.md")).expect("read SUMMARY.md");
    let linked: BTreeSet<String> = summary_links(&summary)
        .iter()
        .map(|l| l.trim_start_matches("./").replace('\\', "/"))
        .collect();
    let src = book_src();
    for file in chapter_files() {
        let rel = file
            .strip_prefix(&src)
            .expect("chapter under src")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "SUMMARY.md" {
            continue;
        }
        assert!(
            linked.contains(&rel),
            "{rel} is in the book source but no SUMMARY entry links to it, so \
             mdBook will not render it and no reader will ever see it."
        );
    }
}

#[test]
fn the_book_configuration_still_refuses_to_invent_chapters() {
    let toml = std::fs::read_to_string(root().join("docs").join("book").join("book.toml"))
        .expect("read book.toml");
    assert!(
        toml.contains("create-missing = false"),
        "create-missing must stay false. With it true, a SUMMARY entry pointing \
         at a missing file is rendered as a blank chapter instead of failing the \
         build — the reason two broken links survived unnoticed."
    );
    for dead in [
        "multilingual",
        "use-hierarchical-outline",
        "fa-github",
        "highlight.js",
    ] {
        assert!(
            !toml.contains(dead),
            "book.toml still names `{dead}`, which mdBook no longer accepts (or \
             which names a file that does not exist). The whole build fails on it."
        );
    }
    for css in toml
        .lines()
        .filter(|l| l.trim_start().starts_with("additional-css"))
    {
        for name in css.split('"').skip(1).step_by(2) {
            assert!(
                root().join("docs").join("book").join(name).is_file(),
                "additional-css names {name}, which is resolved relative to the \
                 book root and is not there. mdBook fails to render."
            );
        }
    }
}

#[test]
fn the_published_book_is_linked_from_the_readme() {
    let readme = std::fs::read_to_string(root().join("README.md")).expect("read README.md");
    assert!(
        readme.contains("nervosys.github.io/AetherShell/book/"),
        "the README must link the published book. It is the first thing anyone \
         reads, and the only question on the repository's one open issue was \
         whether documentation exists at all."
    );
}

#[test]
fn non_vacuity_the_scanner_and_the_tree_both_work() {
    // The link parser must actually find links, and must ignore prose.
    let sample = "# Summary\n\n- [Real](./ai/tools.md)\n- [Nested](./tui/guide.md)\n\
                  \nSee also introduction.md, which is not a link.\n";
    let found = summary_links(sample);
    assert_eq!(
        found,
        vec!["./ai/tools.md".to_string(), "./tui/guide.md".to_string()],
        "the SUMMARY parser is broken, so the tests above prove nothing: {found:?}"
    );

    // A link to a file that is not there must be what the main test rejects.
    let missing = book_src().join("this-chapter-does-not-exist.md");
    assert!(
        !missing.is_file(),
        "a file named to be absent exists; the resolution check is not meaningful"
    );

    // The tree walk must see the real book, not an empty directory.
    let chapters = chapter_files();
    assert!(
        chapters.len() > 40,
        "found only {} chapters under {}; the orphan test would pass vacuously",
        chapters.len(),
        book_src().display()
    );

    let summary = std::fs::read_to_string(book_src().join("SUMMARY.md")).expect("read SUMMARY.md");
    assert!(
        summary_links(&summary).len() > 40,
        "SUMMARY.md yielded almost no links; the resolution test would pass vacuously"
    );
}
