//! Golden-file comparison shared by the jurisdiction test corpora.
//!
//! A golden pins an emitted artefact byte for byte, so any unintended change to a value, a label,
//! a row order or a banner shows up as a diff in `cargo test` rather than in a filer's return.
//!
//! # Regenerating
//!
//! When a change to the emitted artefact is **intentional**, rewrite the corpus with the command
//! each corpus names in its own `regenerate` hint, then read the resulting `git diff` line by line:
//! that diff is the whole point of the corpus, and a golden nobody reviewed is worth no more than
//! no golden at all. A plain `cargo test` never writes to a golden directory.

use std::path::{Path, PathBuf};

pub(crate) struct GoldenCorpus {
    dir: PathBuf,
    regenerate: &'static str,
}

impl GoldenCorpus {
    /// `dir` is repo-relative; `regenerate` is the full shell command that rewrites this corpus.
    pub fn new(dir: &str, regenerate: &'static str) -> GoldenCorpus {
        GoldenCorpus {
            dir: PathBuf::from(dir),
            regenerate,
        }
    }

    pub fn path(&self, name: &str, extension: &str) -> PathBuf {
        self.dir.join(format!("{name}.{extension}"))
    }

    /// Compare against the committed golden, or rewrite it under `UPDATE_GOLDEN`.
    pub fn assert(&self, name: &str, extension: &str, emitted: &str) {
        let path = self.path(name, extension);

        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(&self.dir)
                .unwrap_or_else(|error| panic!("failed to create {}: {error}", self.dir.display()));
            std::fs::write(&path, emitted)
                .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
            return;
        }

        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read the golden {}: {error}. If this case is new, create it with \
                 `{}`, then read the file before committing it.",
                path.display(),
                self.regenerate,
            )
        });

        if expected == emitted {
            return;
        }

        panic!(
            "{}",
            self.describe_difference(name, &path, &expected, emitted)
        );
    }

    /// A golden that no case claims is a golden nobody checks. Catches a renamed case leaving its
    /// file behind, and a hand-added file that never had a test.
    pub fn assert_no_orphans(&self, claimed: &[PathBuf], extensions: &[&str]) {
        let mut orphans = Vec::new();
        for entry in std::fs::read_dir(&self.dir).expect("the golden corpus directory must exist") {
            let entry = entry.unwrap();
            let path = entry.path();
            let known = entry.file_type().is_ok_and(|file_type| file_type.is_file())
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extensions.contains(&extension));
            if known && !claimed.contains(&path) {
                orphans.push(path.display().to_string());
            }
        }
        orphans.sort();

        assert!(
            orphans.is_empty(),
            "golden files with no case claiming them: {}. Delete them or add the case they were \
             written for.",
            orphans.join(", "),
        );
    }

    /// The first differing line, in context. A whole-file dump of two long artefacts tells a reader
    /// nothing they can act on; the line number and its neighbours do.
    fn describe_difference(
        &self,
        name: &str,
        path: &Path,
        expected: &str,
        emitted: &str,
    ) -> String {
        const CONTEXT: usize = 3;

        let expected_lines: Vec<&str> = expected.lines().collect();
        let emitted_lines: Vec<&str> = emitted.lines().collect();

        let first_difference = expected_lines
            .iter()
            .zip(&emitted_lines)
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| expected_lines.len().min(emitted_lines.len()));

        let mut report = format!(
            "golden `{name}` no longer matches the emitted artefact.\n  \
             file: {}\n  \
             first difference at line {} (golden has {} lines, the artefact has {})\n\n",
            path.display(),
            first_difference + 1,
            expected_lines.len(),
            emitted_lines.len(),
        );

        let start = first_difference.saturating_sub(CONTEXT);
        for (offset, line) in expected_lines[start..first_difference].iter().enumerate() {
            report += &format!("  {:>4} | {line}\n", start + offset + 1);
        }

        match expected_lines.get(first_difference) {
            Some(line) => report += &format!("- {:>4} | {line}\n", first_difference + 1),
            None => report += "-      | <the golden ends here>\n",
        }
        match emitted_lines.get(first_difference) {
            Some(line) => report += &format!("+ {:>4} | {line}\n", first_difference + 1),
            None => report += "+      | <the artefact ends here>\n",
        }

        let tail_start = first_difference + 1;
        let tail_end = expected_lines.len().min(tail_start + CONTEXT);
        for (offset, line) in expected_lines[tail_start.min(tail_end)..tail_end]
            .iter()
            .enumerate()
        {
            report += &format!("  {:>4} | {line}\n", tail_start + offset + 1);
        }

        report += &format!(
            "\nIf the change is intended, regenerate with `{}` and review the diff.",
            self.regenerate,
        );
        report
    }
}
