//! Renames a set of files as one unit: every source goes to a unique temporary
//! name first, then each temporary goes to its final name. If any step fails,
//! the completed steps are undone in reverse order, so a folder is never left
//! half-renamed. Swaps (A -> B, B -> A) and case-only renames work because no
//! final name is taken while a source still holds it.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMove {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl FileMove {
    pub fn new(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        FileMove {
            from: from.into(),
            to: to.into(),
        }
    }
}

/// The file system operations the executor needs, so tests can inject failures.
pub trait Renamer {
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
}

pub struct DiskRenamer;

impl Renamer for DiskRenamer {
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn exists(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }
}

#[derive(Debug)]
pub struct RenameError {
    pub message: String,
    /// Steps that could not be undone. Empty when the rollback was complete.
    pub stranded: Vec<FileMove>,
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if self.stranded.is_empty() {
            write!(f, " Nothing was renamed.")
        } else {
            write!(f, " Rollback failed for {} file(s):", self.stranded.len())?;
            for step in &self.stranded {
                write!(f, " {} is at {};", step.from.display(), step.to.display())?;
            }
            Ok(())
        }
    }
}

impl From<RenameError> for String {
    fn from(error: RenameError) -> Self {
        error.to_string()
    }
}

pub fn execute(moves: &[FileMove]) -> Result<(), RenameError> {
    execute_with(moves, &mut DiskRenamer)
}

pub fn execute_with<R: Renamer>(moves: &[FileMove], renamer: &mut R) -> Result<(), RenameError> {
    let moves: Vec<&FileMove> = moves.iter().filter(|m| m.from != m.to).collect();
    validate(&moves).map_err(|message| RenameError {
        message,
        stranded: Vec::new(),
    })?;

    let mut done: Vec<FileMove> = Vec::with_capacity(moves.len() * 2);
    let mut reserved: HashSet<PathBuf> = HashSet::new();
    let mut temps: Vec<PathBuf> = Vec::with_capacity(moves.len());

    for (index, step) in moves.iter().enumerate() {
        let temp = temp_path_for(&step.from, index, &reserved, renamer);
        reserved.insert(temp.clone());
        if let Err(e) = renamer.rename(&step.from, &temp) {
            return Err(roll_back(
                renamer,
                done,
                format!("Could not rename {}: {}.", step.from.display(), e),
            ));
        }
        done.push(FileMove::new(step.from.clone(), temp.clone()));
        temps.push(temp);
    }

    for (step, temp) in moves.iter().zip(&temps) {
        if renamer.exists(&step.to) {
            return Err(roll_back(
                renamer,
                done,
                format!("{} already exists.", step.to.display()),
            ));
        }
        if let Err(e) = renamer.rename(temp, &step.to) {
            return Err(roll_back(
                renamer,
                done,
                format!(
                    "Could not rename {} to {}: {}.",
                    step.from.display(),
                    step.to.display(),
                    e
                ),
            ));
        }
        done.push(FileMove::new(temp.clone(), step.to.clone()));
    }

    Ok(())
}

fn validate(moves: &[&FileMove]) -> Result<(), String> {
    let mut sources = HashSet::new();
    let mut targets = HashSet::new();
    for step in moves {
        if step.from.parent().is_none() || step.to.parent().is_none() {
            return Err(format!("Invalid path: {}.", step.from.display()));
        }
        if !sources.insert(&step.from) {
            return Err(format!("{} is renamed twice.", step.from.display()));
        }
        if !targets.insert(&step.to) {
            return Err(format!(
                "More than one file would be named {}.",
                step.to.display()
            ));
        }
    }
    Ok(())
}

fn temp_path_for<R: Renamer>(
    source: &Path,
    index: usize,
    reserved: &HashSet<PathBuf>,
    renamer: &R,
) -> PathBuf {
    let parent = source.parent().unwrap_or(Path::new(""));
    let pid = std::process::id();
    let mut attempt = 0usize;
    loop {
        let candidate = parent.join(format!(".rrrename-{}-{}-{}.tmp", pid, index, attempt));
        if !reserved.contains(&candidate) && !renamer.exists(&candidate) {
            return candidate;
        }
        attempt += 1;
    }
}

fn roll_back<R: Renamer>(renamer: &mut R, done: Vec<FileMove>, message: String) -> RenameError {
    let mut stranded = Vec::new();
    for step in done.into_iter().rev() {
        if let Err(e) = renamer.rename(&step.to, &step.from) {
            log::error!(
                "Rollback of {} -> {} failed: {}",
                step.from.display(),
                step.to.display(),
                e
            );
            stranded.push(step);
        }
    }
    RenameError { message, stranded }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingRenamer {
        calls: usize,
        fail_at: Vec<usize>,
    }

    impl FailingRenamer {
        fn failing_at(fail_at: &[usize]) -> Self {
            FailingRenamer {
                calls: 0,
                fail_at: fail_at.to_vec(),
            }
        }
    }

    impl Renamer for FailingRenamer {
        fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
            let call = self.calls;
            self.calls += 1;
            if self.fail_at.contains(&call) {
                return Err(io::Error::other("injected failure"));
            }
            fs::rename(from, to)
        }

        fn exists(&self, path: &Path) -> bool {
            path.exists()
        }
    }

    fn write(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, name).unwrap();
        path
    }

    fn listing(dir: &Path) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    fs::read_to_string(e.path()).unwrap(),
                )
            })
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn renames_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.jpg");
        let b = write(dir.path(), "a.jpg.rrdata");
        execute(&[
            FileMove::new(&a, dir.path().join("x.jpg")),
            FileMove::new(&b, dir.path().join("x.jpg.rrdata")),
        ])
        .unwrap();
        assert_eq!(
            listing(dir.path()),
            vec![
                ("x.jpg".into(), "a.jpg".into()),
                ("x.jpg.rrdata".into(), "a.jpg.rrdata".into())
            ]
        );
    }

    #[test]
    fn swaps_and_chains_work() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "1.jpg");
        let b = write(dir.path(), "2.jpg");
        let c = write(dir.path(), "3.jpg");
        execute(&[
            FileMove::new(&a, &b),
            FileMove::new(&b, &c),
            FileMove::new(&c, &a),
        ])
        .unwrap();
        assert_eq!(
            listing(dir.path()),
            vec![
                ("1.jpg".into(), "3.jpg".into()),
                ("2.jpg".into(), "1.jpg".into()),
                ("3.jpg".into(), "2.jpg".into())
            ]
        );
    }

    #[test]
    fn case_only_rename_works() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "img.jpg");
        execute(&[FileMove::new(&a, dir.path().join("IMG.jpg"))]).unwrap();
        assert_eq!(
            listing(dir.path()),
            vec![("IMG.jpg".into(), "img.jpg".into())]
        );
    }

    #[test]
    fn refuses_duplicate_targets_before_touching_anything() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.jpg");
        let b = write(dir.path(), "b.jpg");
        let target = dir.path().join("x.jpg");
        let error = execute(&[FileMove::new(&a, &target), FileMove::new(&b, &target)]).unwrap_err();
        assert!(error.message.contains("More than one file"));
        assert_eq!(
            listing(dir.path()),
            vec![
                ("a.jpg".into(), "a.jpg".into()),
                ("b.jpg".into(), "b.jpg".into())
            ]
        );
    }

    #[test]
    fn never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.jpg");
        let b = write(dir.path(), "b.jpg");
        write(dir.path(), "taken.jpg");
        let error = execute(&[
            FileMove::new(&a, dir.path().join("free.jpg")),
            FileMove::new(&b, dir.path().join("taken.jpg")),
        ])
        .unwrap_err();
        assert!(error.message.contains("already exists"));
        assert!(error.stranded.is_empty());
        assert_eq!(
            listing(dir.path()),
            vec![
                ("a.jpg".into(), "a.jpg".into()),
                ("b.jpg".into(), "b.jpg".into()),
                ("taken.jpg".into(), "taken.jpg".into())
            ]
        );
    }

    #[test]
    fn rolls_back_after_a_failure_at_any_step() {
        // 3 moves = 3 steps to temporary names + 3 steps to final names.
        for fail_at in 0..6 {
            let dir = tempfile::tempdir().unwrap();
            let files: Vec<PathBuf> = ["a.arw", "a.jpg", "a.arw.rrdata"]
                .iter()
                .map(|n| write(dir.path(), n))
                .collect();
            let before = listing(dir.path());
            let moves: Vec<FileMove> = files
                .iter()
                .map(|f| {
                    let name = f
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .replacen('a', "b", 1);
                    FileMove::new(f, dir.path().join(name))
                })
                .collect();
            let error = execute_with(&moves, &mut FailingRenamer::failing_at(&[fail_at]))
                .expect_err("injected failure must surface");
            assert!(error.message.contains("injected failure"), "{}", error);
            assert!(error.stranded.is_empty());
            assert_eq!(listing(dir.path()), before, "failure at step {}", fail_at);
        }
    }

    #[test]
    fn reports_files_a_failed_rollback_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.jpg");
        let b = write(dir.path(), "b.jpg");
        // Steps: 0 a->tmp, 1 b->tmp, 2 tmp->x (fails), rollback: 3 tmp->b (fails), 4 tmp->a.
        let error = execute_with(
            &[
                FileMove::new(&a, dir.path().join("x.jpg")),
                FileMove::new(&b, dir.path().join("y.jpg")),
            ],
            &mut FailingRenamer::failing_at(&[2, 3]),
        )
        .unwrap_err();
        assert_eq!(error.stranded.len(), 1);
        assert_eq!(error.stranded[0].from, b);
        assert!(error.to_string().contains("Rollback failed"));
        assert!(a.exists());
    }

    #[test]
    fn skips_no_op_moves() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.jpg");
        execute_with(
            &[FileMove::new(&a, &a)],
            &mut FailingRenamer::failing_at(&[0]),
        )
        .unwrap();
        assert!(a.exists());
    }
}
