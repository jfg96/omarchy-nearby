use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// What one receive has put on disk.
#[derive(Debug, Default, PartialEq)]
pub struct SessionFiles {
    files: Vec<PathBuf>,
    directories: BTreeSet<PathBuf>,
}

/// What each receive in progress has put on disk, so that a cancel from the
/// user can leave the folder as it was. Receives that end any other way keep
/// their files.
#[derive(Debug)]
pub struct ReceivedFiles {
    save_dir: PathBuf,
    sessions: HashMap<String, SessionFiles>,
}

impl ReceivedFiles {
    pub fn new(save_dir: PathBuf) -> Self {
        Self {
            save_dir,
            sessions: HashMap::new(),
        }
    }

    /// The receiver creates a file's folders before its first byte arrives, so
    /// a file still in progress can leave folders behind too.
    pub fn note_upload(&mut self, session: &str, remote_name: &str) {
        if let Some(parent) = self.save_dir.join(remote_name).parent() {
            let parent = parent.to_path_buf();
            self.session(session).directories.insert(parent);
        }
    }

    pub fn note_saved(&mut self, session: &str, path: PathBuf) {
        let parent = path.parent().map(Path::to_path_buf);
        let files = self.session(session);
        files.files.push(path);
        files.directories.extend(parent);
    }

    pub fn finish(&mut self, session: &str) -> SessionFiles {
        self.sessions.remove(session).unwrap_or_default()
    }

    fn session(&mut self, session: &str) -> &mut SessionFiles {
        self.sessions.entry(session.to_owned()).or_default()
    }
}

/// Remove what a cancelled receive saved. Every path is one the receiver
/// created for this session, as it never replaces an existing file, and
/// folders go only once they are empty, so nothing else in them is touched.
pub fn discard(save_dir: &Path, session: SessionFiles) {
    for file in &session.files {
        if !file.starts_with(save_dir) {
            continue;
        }
        let regular = fs::symlink_metadata(file).is_ok_and(|meta| meta.is_file());
        if regular && let Err(error) = fs::remove_file(file) {
            eprintln!("could not remove a cancelled file: {error}");
        }
    }
    for directory in &session.directories {
        let mut current = directory.as_path();
        while current != save_dir && current.starts_with(save_dir) {
            if fs::remove_dir(current).is_err() {
                break;
            }
            let Some(parent) = current.parent() else {
                break;
            };
            current = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "omarchy-nearby-received-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"bytes").unwrap();
    }

    #[test]
    fn a_cancelled_receive_leaves_the_folder_as_it_was() {
        let save = folder("whole");
        let mine = save.join("mine.txt");
        write(&mine);
        let mut received = ReceivedFiles::new(save.clone());
        for name in ["photo.jpg", "Trip/day 1/a.jpg", "Trip/day 2/b.jpg"] {
            let path = save.join(name);
            write(&path);
            received.note_upload("s", name);
            received.note_saved("s", path);
        }

        discard(&save, received.finish("s"));

        let left: Vec<_> = fs::read_dir(&save)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(left, vec![mine], "only what was there before may remain");
        fs::remove_dir_all(save).unwrap();
    }

    #[test]
    fn folders_created_for_a_file_still_in_progress_are_removed() {
        let save = folder("in-progress");
        fs::create_dir_all(save.join("Trip/day 1")).unwrap();
        let mut received = ReceivedFiles::new(save.clone());
        received.note_upload("s", "Trip/day 1/a.jpg");

        discard(&save, received.finish("s"));

        assert!(!save.join("Trip").exists());
        assert!(save.exists(), "the save folder itself always stays");
        fs::remove_dir_all(save).unwrap();
    }

    #[test]
    fn a_shared_folder_keeps_what_was_already_in_it() {
        let save = folder("merged");
        let mine = save.join("Trip/mine.jpg");
        write(&mine);
        let collision = save.join("Trip/mine (1).jpg");
        write(&collision);
        let mut received = ReceivedFiles::new(save.clone());
        received.note_saved("s", collision.clone());

        discard(&save, received.finish("s"));

        assert!(mine.exists(), "a merged folder keeps the user's file");
        assert!(!collision.exists());
        fs::remove_dir_all(save).unwrap();
    }

    #[test]
    fn nothing_outside_the_save_folder_is_touched() {
        let save = folder("bounds");
        let outside = folder("bounds-outside");
        let file = outside.join("keep.txt");
        write(&file);
        let mut received = ReceivedFiles::new(save.clone());
        received.note_saved("s", file.clone());

        discard(&save, received.finish("s"));

        assert!(file.exists());
        fs::remove_dir_all(save).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn a_symlink_recorded_in_place_of_a_file_is_left_alone() {
        let save = folder("symlink");
        let target = save.join("target.txt");
        write(&target);
        let link = save.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut received = ReceivedFiles::new(save.clone());
        received.note_saved("s", link.clone());

        discard(&save, received.finish("s"));

        assert!(link.is_symlink());
        assert!(target.exists());
        fs::remove_dir_all(save).unwrap();
    }

    #[test]
    fn sessions_are_kept_apart_and_finish_once() {
        let save = folder("sessions");
        let first = save.join("first.txt");
        let second = save.join("second.txt");
        write(&first);
        write(&second);
        let mut received = ReceivedFiles::new(save.clone());
        received.note_saved("one", first.clone());
        received.note_saved("two", second.clone());

        discard(&save, received.finish("one"));

        assert!(!first.exists());
        assert!(second.exists(), "another session's files must stay");
        assert_eq!(received.finish("one"), SessionFiles::default());
        fs::remove_dir_all(save).unwrap();
    }
}
