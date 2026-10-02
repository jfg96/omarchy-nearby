use anyhow::{Context, Result, anyhow};
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// `user-dirs.dirs` holds a handful of short lines; anything larger is not one.
const MAX_USER_DIRS_BYTES: usize = 16 * 1024;

/// Where received files are saved, following the XDG user directories.
pub fn download_dir(home: &Path) -> PathBuf {
    download_dir_from(
        home,
        std::env::var_os("XDG_DOWNLOAD_DIR"),
        std::env::var_os("XDG_CONFIG_HOME"),
    )
}

fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

fn download_dir_from(
    home: &Path,
    env_download: Option<OsString>,
    env_config: Option<OsString>,
) -> PathBuf {
    if let Some(path) = absolute(env_download) {
        return path;
    }
    // The XDG base directory specification says relative values are invalid
    // and must be ignored.
    let config = absolute(env_config).unwrap_or_else(|| home.join(".config"));
    let path = config.join("user-dirs.dirs");
    match read_user_dirs(&path) {
        Ok(Some(text)) => {
            if let Some(path) = parse_download_dir(&text, home) {
                return path;
            }
        }
        Ok(None) => {}
        Err(error) => eprintln!("ignoring {}: {error:#}", path.display()),
    }
    home.join("Downloads")
}

/// Read `user-dirs.dirs` without trusting what the path names.
///
/// Symlinks are followed on purpose: this is user configuration, and dotfile
/// managers commonly link exactly this file. What is opened must still be a
/// small regular file owned by the user or root, and a FIFO is rejected
/// without waiting for a writer.
fn read_user_dirs(path: &Path) -> Result<Option<String>> {
    let file = match File::options()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("could not open"),
    };
    let metadata = file.metadata().context("could not inspect")?;
    if !metadata.is_file() {
        return Err(anyhow!("not a regular file"));
    }
    let uid = unsafe { libc::geteuid() };
    if metadata.uid() != uid && metadata.uid() != 0 {
        return Err(anyhow!("owned by another user"));
    }
    let mut data = Vec::new();
    file.take((MAX_USER_DIRS_BYTES + 1) as u64)
        .read_to_end(&mut data)
        .context("could not read")?;
    if data.len() > MAX_USER_DIRS_BYTES {
        return Err(anyhow!("larger than 16 KiB"));
    }
    String::from_utf8(data)
        .map(Some)
        .map_err(|_| anyhow!("not UTF-8"))
}

/// The `XDG_DOWNLOAD_DIR` entry, which the format allows only as `$HOME/...`
/// or an absolute path.
fn parse_download_dir(text: &str, home: &Path) -> Option<PathBuf> {
    let raw = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("XDG_DOWNLOAD_DIR="))?;
    let value = raw.trim().strip_prefix('"')?.strip_suffix('"')?;
    if value == "$HOME" {
        return Some(home.to_path_buf());
    }
    if let Some(rest) = value.strip_prefix("$HOME/") {
        return Some(home.join(rest));
    }
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn test_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "omarchy-nearby-downloads-{name}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn resolve(home: &Path, config: &Path) -> PathBuf {
        download_dir_from(home, None, Some(config.as_os_str().to_owned()))
    }

    #[test]
    fn configured_download_directory_is_used() {
        let config = test_directory("configured");
        let home = Path::new("/home/someone");
        fs::write(
            config.join("user-dirs.dirs"),
            "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Incoming\"\n",
        )
        .unwrap();
        assert_eq!(resolve(home, &config), home.join("Incoming"));
        fs::write(
            config.join("user-dirs.dirs"),
            "XDG_DOWNLOAD_DIR=\"/data/in\"\n",
        )
        .unwrap();
        assert_eq!(resolve(home, &config), PathBuf::from("/data/in"));
        fs::remove_dir_all(config).unwrap();
    }

    #[test]
    fn relative_locations_are_ignored() {
        let config = test_directory("relative");
        let home = Path::new("/home/someone");
        fs::write(
            config.join("user-dirs.dirs"),
            "XDG_DOWNLOAD_DIR=\"Incoming\"\n",
        )
        .unwrap();
        assert_eq!(resolve(home, &config), home.join("Downloads"));
        assert_eq!(
            download_dir_from(home, Some("relative".into()), Some("also-relative".into())),
            home.join("Downloads")
        );
        assert_eq!(
            download_dir_from(home, Some("/data/env".into()), None),
            PathBuf::from("/data/env")
        );
        fs::remove_dir_all(config).unwrap();
    }

    #[test]
    fn oversized_and_non_regular_user_dirs_are_ignored() {
        let config = test_directory("oversized");
        let home = Path::new("/home/someone");
        let mut text = " ".repeat(MAX_USER_DIRS_BYTES);
        text.push_str("\nXDG_DOWNLOAD_DIR=\"$HOME/Incoming\"\n");
        fs::write(config.join("user-dirs.dirs"), text).unwrap();
        assert!(read_user_dirs(&config.join("user-dirs.dirs")).is_err());
        assert_eq!(resolve(home, &config), home.join("Downloads"));

        fs::remove_file(config.join("user-dirs.dirs")).unwrap();
        fs::create_dir(config.join("user-dirs.dirs")).unwrap();
        assert!(read_user_dirs(&config.join("user-dirs.dirs")).is_err());
        assert_eq!(resolve(home, &config), home.join("Downloads"));
        fs::remove_dir_all(config).unwrap();
    }

    #[test]
    fn symlinked_user_dirs_is_followed() {
        let config = test_directory("symlink");
        let home = Path::new("/home/someone");
        fs::write(config.join("real"), "XDG_DOWNLOAD_DIR=\"$HOME/Linked\"\n").unwrap();
        std::os::unix::fs::symlink(config.join("real"), config.join("user-dirs.dirs")).unwrap();
        assert_eq!(resolve(home, &config), home.join("Linked"));
        fs::remove_dir_all(config).unwrap();
    }

    #[test]
    fn fifo_child() {
        if let Some(config) = std::env::var_os("NEARBY_TEST_USER_DIRS_FIFO") {
            let home = Path::new("/home/someone");
            assert_eq!(resolve(home, Path::new(&config)), home.join("Downloads"));
        }
    }

    #[test]
    fn fifo_user_dirs_is_ignored_without_blocking() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let config = test_directory("fifo");
        let path = config.join("user-dirs.dirs");
        let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "downloads::tests::fifo_child"])
            .env("NEARBY_TEST_USER_DIRS_FIFO", &config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        fs::remove_dir_all(config).unwrap();
        assert!(
            status.is_some_and(|status| status.success()),
            "reading a FIFO user-dirs.dirs blocked or failed"
        );
    }
}
