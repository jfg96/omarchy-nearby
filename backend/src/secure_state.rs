//! Linux filesystem transactions for Nearby's private receiver state.
//!
//! Walk from `/` without following any component. System ancestors may belong
//! to root; other ancestors and the final private directory must belong to the
//! effective user. Once opened, all state-file operations stay relative to the
//! trusted directory descriptor. This does not defend against a compromised
//! process running as that same user.

use anyhow::{Context, Result, anyhow};
use std::ffi::{CString, OsStr};
use std::fs::{File, Metadata, Permissions};
use std::io::{ErrorKind, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path};

const MODE_PRIVATE_DIR: u32 = 0o700;
const MODE_PRIVATE_FILE: u32 = 0o600;
const TEMP_ATTEMPTS: usize = 16;

pub struct StateDir {
    directory: File,
}

fn name_cstr(name: &OsStr) -> Result<CString> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(anyhow!("invalid Nearby state entry name"));
    }
    CString::new(bytes).context("Nearby state path contains a NUL byte")
}

fn openat_file(directory: &File, name: &OsStr, flags: i32, mode: libc::mode_t) -> Result<File> {
    let name = name_cstr(name)?;
    // SAFETY: both descriptors and the NUL-terminated name are valid for this
    // call; ownership of a successful descriptor moves immediately to File.
    let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags, mode) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn mkdirat(directory: &File, name: &OsStr) -> Result<()> {
    let name = name_cstr(name)?;
    // SAFETY: the directory descriptor and NUL-terminated name remain valid.
    let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), MODE_PRIVATE_DIR) };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn renameat(directory: &File, from: &OsStr, to: &OsStr) -> Result<()> {
    let from = name_cstr(from)?;
    let to = name_cstr(to)?;
    // SAFETY: both names and the held directory descriptor are valid.
    let result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
        )
    };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn unlinkat(directory: &File, name: &OsStr) -> Result<()> {
    let name = name_cstr(name)?;
    // SAFETY: the held directory descriptor and NUL-terminated name are valid.
    let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn valid_directory_owner(metadata: &Metadata, expected_uid: u32, private: bool) -> Result<()> {
    if !metadata.is_dir() {
        return Err(anyhow!("Nearby state path component is not a directory"));
    }
    if metadata.uid() != expected_uid && (private || metadata.uid() != 0) {
        return Err(anyhow!("Nearby state directory has an unexpected owner"));
    }
    // In a non-sticky writable ancestor, another user can rename an entry
    // after it is checked. Sticky directories such as /tmp protect entries
    // owned by this user or root; the next opened component is owner-checked.
    if !private && metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0 {
        return Err(anyhow!("Nearby state ancestor is writable by another user"));
    }
    Ok(())
}

pub fn validate_owned_regular_file(metadata: &Metadata, expected_uid: u32) -> Result<()> {
    if !metadata.is_file() {
        return Err(anyhow!("Nearby state entry must be a regular file"));
    }
    if metadata.uid() != expected_uid {
        return Err(anyhow!("Nearby state entry has an unexpected owner"));
    }
    Ok(())
}

fn random_temp_name(target: &str) -> Result<String> {
    let mut random = [0u8; 16];
    let mut filled = 0;
    while filled < random.len() {
        // SAFETY: the slice points to writable memory of the supplied length.
        let count = unsafe {
            libc::getrandom(
                random[filled..].as_mut_ptr().cast(),
                random.len() - filled,
                0,
            )
        };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(error).context("could not generate state staging name");
        }
        if count == 0 {
            return Err(anyhow!("random source returned no bytes"));
        }
        filled += count as usize;
    }
    let mut name = format!(".{target}.tmp-");
    for byte in random {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(name)
}

impl StateDir {
    pub fn open_or_create(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(anyhow!("Nearby state directory must be absolute"));
        }
        // Path::components normalizes interior `.` segments, so inspect the
        // original bytes before walking any component.
        if path
            .as_os_str()
            .as_bytes()
            .split(|byte| *byte == b'/')
            .any(|part| part == b"." || part == b"..")
        {
            return Err(anyhow!("Nearby state directory contains dot components"));
        }
        let components: Vec<_> = path.components().collect();
        if components.len() < 2 {
            return Err(anyhow!("Nearby state directory must be below root"));
        }
        if components
            .iter()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(anyhow!("Nearby state directory contains dot components"));
        }

        // `/` is a fixed kernel root; every later component is opened relative
        // to the preceding descriptor without following its final symlink.
        let mut directory = File::open("/").context("could not open filesystem root")?;
        let expected_uid = unsafe { libc::geteuid() };
        valid_directory_owner(&directory.metadata()?, expected_uid, false)?;
        for (index, component) in components.iter().enumerate().skip(1) {
            let Component::Normal(name) = component else {
                return Err(anyhow!("invalid Nearby state directory component"));
            };
            let child = match openat_file(
                &directory,
                name,
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0,
            ) {
                Ok(child) => child,
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == ErrorKind::NotFound) =>
                {
                    match mkdirat(&directory, name) {
                        Ok(()) => {}
                        Err(error)
                            if error
                                .downcast_ref::<std::io::Error>()
                                .is_some_and(|error| error.kind() == ErrorKind::AlreadyExists) => {}
                        Err(error) => {
                            return Err(error).context("could not create Nearby state directory");
                        }
                    }
                    openat_file(
                        &directory,
                        name,
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        0,
                    )
                    .context("could not open created Nearby state directory")?
                }
                Err(error) => return Err(error).context("could not open Nearby state directory"),
            };
            let private = index == components.len() - 1;
            valid_directory_owner(&child.metadata()?, expected_uid, private)?;
            if private {
                child
                    .set_permissions(Permissions::from_mode(MODE_PRIVATE_DIR))
                    .context("could not protect Nearby state directory")?;
            }
            directory = child;
        }
        Ok(Self { directory })
    }

    pub fn open_regular(&self, name: &str) -> Result<Option<(File, Metadata)>> {
        let file = match openat_file(
            &self.directory,
            OsStr::new(name),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            0,
        ) {
            Ok(file) => file,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == ErrorKind::NotFound) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error).context("could not open Nearby state file"),
        };
        let metadata = file
            .metadata()
            .context("could not inspect opened Nearby state file")?;
        validate_owned_regular_file(&metadata, unsafe { libc::geteuid() })?;
        Ok(Some((file, metadata)))
    }

    pub fn replace(&self, name: &str, bytes: &[u8]) -> Result<()> {
        self.replace_with_names(name, bytes, || random_temp_name(name))
    }

    fn replace_with_names(
        &self,
        name: &str,
        bytes: &[u8],
        next_name: impl FnMut() -> Result<String>,
    ) -> Result<()> {
        self.replace_with_names_and_publish(name, bytes, next_name, renameat)
    }

    fn replace_with_names_and_publish(
        &self,
        name: &str,
        bytes: &[u8],
        next_name: impl FnMut() -> Result<String>,
        publish: impl FnOnce(&File, &OsStr, &OsStr) -> Result<()>,
    ) -> Result<()> {
        self.replace_with_hooks(name, bytes, next_name, publish, File::sync_all)
    }

    fn replace_with_hooks(
        &self,
        name: &str,
        bytes: &[u8],
        mut next_name: impl FnMut() -> Result<String>,
        publish: impl FnOnce(&File, &OsStr, &OsStr) -> Result<()>,
        sync_directory: impl FnOnce(&File) -> std::io::Result<()>,
    ) -> Result<()> {
        name_cstr(OsStr::new(name))?;
        // Fail closed for an existing unsafe object before attempting to
        // replace its directory entry. No pathname-based check is used.
        self.open_regular(name)?;
        let mut temporary = None;
        let mut file = None;
        for _ in 0..TEMP_ATTEMPTS {
            let candidate = next_name()?;
            match openat_file(
                &self.directory,
                OsStr::new(&candidate),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                MODE_PRIVATE_FILE,
            ) {
                Ok(created) => {
                    temporary = Some(candidate);
                    file = Some(created);
                    break;
                }
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == ErrorKind::AlreadyExists) =>
                {
                    continue;
                }
                Err(error) => {
                    return Err(error).context("could not create temporary Nearby state file");
                }
            }
        }
        let temporary = temporary
            .ok_or_else(|| anyhow!("could not find an unused Nearby state staging name"))?;
        let mut file = file.expect("a staging name has an opened file");
        let result = (|| -> Result<()> {
            // openat's creation mode is still reduced by umask. Set the exact
            // final mode on the opened object before writing any state bytes.
            file.set_permissions(Permissions::from_mode(MODE_PRIVATE_FILE))
                .context("could not protect temporary Nearby state file")?;
            file.write_all(bytes)
                .context("could not write Nearby state file")?;
            file.sync_all()
                .context("could not flush Nearby state file")?;
            Ok(())
        })();
        drop(file);
        if let Err(error) = result {
            let _ = unlinkat(&self.directory, OsStr::new(&temporary));
            return Err(error);
        }
        if let Err(error) = publish(&self.directory, OsStr::new(&temporary), OsStr::new(name)) {
            let _ = unlinkat(&self.directory, OsStr::new(&temporary));
            return Err(error).context("could not publish Nearby state file");
        }
        // The temporary name no longer belongs to this transaction. In
        // particular, a later directory-sync failure must not unlink it.
        sync_directory(&self.directory).context("could not flush Nearby state directory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{FileTypeExt, symlink};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn directory(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "nearby-secure-state-{name}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn private_directory_is_created_and_mode_repaired() {
        let base = directory("mode");
        let path = base.join(".local/state/omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        assert_eq!(
            state.directory.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::set_permissions(&path, Permissions::from_mode(0o755)).unwrap();
        let reopened = StateDir::open_or_create(&path).unwrap();
        assert_eq!(
            reopened.directory.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn final_and_intermediate_symlinks_are_rejected() {
        let base = directory("links");
        let target = base.join("target");
        fs::create_dir_all(&target).unwrap();
        let final_link = base.join("omarchy-nearby");
        symlink(&target, &final_link).unwrap();
        assert!(StateDir::open_or_create(&final_link).is_err());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        let intermediate = base.join("linked");
        symlink(&target, &intermediate).unwrap();
        assert!(StateDir::open_or_create(&intermediate.join("omarchy-nearby")).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn opened_directory_stays_bound_after_path_substitution() {
        let base = directory("binding");
        let path = base.join("omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        let moved = base.join("moved");
        fs::rename(&path, &moved).unwrap();
        fs::create_dir(&path).unwrap();
        state.replace("settings.json", b"safe").unwrap();
        assert_eq!(fs::read(moved.join("settings.json")).unwrap(), b"safe");
        assert!(!path.join("settings.json").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn owner_validation_rejects_foreign_private_dir_and_file() {
        let path = directory("owner").join("omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        let uid = unsafe { libc::geteuid() };
        assert!(
            valid_directory_owner(
                &state.directory.metadata().unwrap(),
                uid.wrapping_add(1),
                true
            )
            .is_err()
        );
        state.replace("settings.json", b"safe").unwrap();
        let (_, metadata) = state.open_regular("settings.json").unwrap().unwrap();
        assert!(validate_owned_regular_file(&metadata, uid.wrapping_add(1)).is_err());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invalid_directory_paths_fail_closed() {
        let base = directory("invalid");
        fs::create_dir_all(&base).unwrap();
        let regular = base.join("omarchy-nearby");
        fs::write(&regular, b"keep").unwrap();
        assert!(StateDir::open_or_create(&regular).is_err());
        assert_eq!(fs::read(&regular).unwrap(), b"keep");
        assert!(StateDir::open_or_create(Path::new("relative/state")).is_err());
        assert!(StateDir::open_or_create(&base.join("../other")).is_err());
        assert!(StateDir::open_or_create(&base.join("./other")).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn writable_non_sticky_ancestor_is_rejected() {
        let base = directory("writable-ancestor");
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, Permissions::from_mode(0o777)).unwrap();
        let private = base.join("omarchy-nearby");
        assert!(StateDir::open_or_create(&private).is_err());
        assert!(!private.exists());
        fs::set_permissions(&base, Permissions::from_mode(0o1777)).unwrap();
        assert!(StateDir::open_or_create(&private).is_ok());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn restrictive_umask_child() {
        let Some(path) = std::env::var_os("NEARBY_TEST_RESTRICTIVE_UMASK") else {
            return;
        };
        struct RestoreUmask(libc::mode_t);
        impl Drop for RestoreUmask {
            fn drop(&mut self) {
                unsafe { libc::umask(self.0) };
            }
        }
        let original = unsafe { libc::umask(0o777) };
        let _restore = RestoreUmask(original);
        let state = StateDir::open_or_create(Path::new(&path)).unwrap();
        state.replace("settings.json", b"private").unwrap();
        let (_, metadata) = state.open_regular("settings.json").unwrap().unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn restrictive_umask_still_publishes_mode_0600() {
        let base = directory("umask");
        let private = base.join("omarchy-nearby");
        StateDir::open_or_create(&private).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "secure_state::tests::restrictive_umask_child"])
            .env("NEARBY_TEST_RESTRICTIVE_UMASK", &private)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            fs::metadata(private.join("settings.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn random_exclusive_staging_retries_without_touching_collisions() {
        let base = directory("collision");
        let path = base.join("omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        let target = base.join("target");
        fs::write(&target, b"keep").unwrap();
        let first = ".settings.json.tmp-collision-link";
        let second = ".settings.json.tmp-collision-fifo";
        symlink(&target, path.join(first)).unwrap();
        let c_name = name_cstr(OsStr::new(second)).unwrap();
        assert_eq!(
            unsafe { libc::mkfifoat(state.directory.as_raw_fd(), c_name.as_ptr(), 0o600) },
            0
        );
        let mut names = [first, second, ".settings.json.tmp-good"].into_iter();
        state
            .replace_with_names("settings.json", b"new", || Ok(names.next().unwrap().into()))
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert_eq!(fs::read_link(path.join(first)).unwrap(), target);
        assert!(
            fs::metadata(path.join(second))
                .unwrap()
                .file_type()
                .is_fifo()
        );
        assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"new");
        assert!(!path.join(".settings.json.tmp-good").exists());
        let random_a = random_temp_name("settings.json").unwrap();
        let random_b = random_temp_name("settings.json").unwrap();
        assert_ne!(random_a, random_b);
        assert!(random_a.starts_with(".settings.json.tmp-"));
        assert_eq!(random_a.len(), ".settings.json.tmp-".len() + 32);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn failed_publication_preserves_previous_file_and_cleans_only_own_temp() {
        let base = directory("publish");
        let path = base.join("omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        state.replace("settings.json", b"previous").unwrap();
        let unrelated = path.join(".settings.json.tmp-other");
        fs::write(&unrelated, b"other").unwrap();
        let error = state.replace_with_names_and_publish(
            "settings.json",
            b"replacement",
            || Ok(".settings.json.tmp-owned".into()),
            |_, _, _| Err(anyhow!("injected publication failure")),
        );
        assert!(error.is_err());
        assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"previous");
        assert_eq!(fs::read(unrelated).unwrap(), b"other");
        assert!(!path.join(".settings.json.tmp-owned").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn failed_directory_sync_does_not_unlink_a_reused_temp_name() {
        let base = directory("sync-failure");
        let path = base.join("omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        let temporary = ".settings.json.tmp-owned";
        let result = state.replace_with_hooks(
            "settings.json",
            b"published",
            || Ok(temporary.into()),
            renameat,
            |directory| {
                let mut reused = openat_file(
                    directory,
                    OsStr::new(temporary),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                    MODE_PRIVATE_FILE,
                )
                .unwrap();
                reused.write_all(b"another entry").unwrap();
                Err(std::io::Error::other("injected directory sync failure"))
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"published");
        assert_eq!(fs::read(path.join(temporary)).unwrap(), b"another entry");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn destination_symlink_is_rejected_without_touching_target() {
        let base = directory("destination-link");
        let path = base.join("omarchy-nearby");
        let state = StateDir::open_or_create(&path).unwrap();
        let target = base.join("target");
        fs::write(&target, b"keep").unwrap();
        symlink(&target, path.join("settings.json")).unwrap();
        assert!(state.replace("settings.json", b"replacement").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
        fs::remove_dir_all(base).unwrap();
    }
}
