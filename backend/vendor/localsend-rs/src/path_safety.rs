use crate::error::{LocalSendError, Result};
use std::path::{Component, Path, PathBuf};

pub(crate) fn safe_join(base: &Path, remote_name: &str) -> Result<PathBuf> {
    if remote_name.is_empty()
        || remote_name.contains('\0')
        || remote_name.contains('\\')
        || remote_name.contains(':')
    {
        return Err(LocalSendError::invalid_file(format!(
            "Unsafe remote file name: {}",
            remote_name
        )));
    }

    let remote_path = Path::new(remote_name);
    if remote_path.is_absolute() {
        return Err(LocalSendError::invalid_file(format!(
            "Unsafe absolute remote file name: {}",
            remote_name
        )));
    }

    let mut relative = PathBuf::new();
    for component in remote_path.components() {
        match component {
            Component::Normal(part) => {
                relative.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(LocalSendError::invalid_file(format!(
                    "Unsafe remote file name: {}",
                    remote_name
                )));
            }
        }
    }

    if relative.as_os_str().is_empty() {
        return Err(LocalSendError::invalid_file(format!(
            "Unsafe empty remote file name: {}",
            remote_name
        )));
    }

    // Remote file names may carry relative directory components for folder
    // transfers. Ensure no component inside base is an existing symlink to
    // prevent symlinked-parent directory traversal escapes.
    let mut check_path = base.to_path_buf();
    for part in relative.components() {
        check_path.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&check_path) {
            if meta.file_type().is_symlink() {
                return Err(LocalSendError::invalid_file(format!(
                    "Unsafe symlink in remote path: {}",
                    remote_name
                )));
            }
        }
    }

    Ok(base.join(relative))
}

#[cfg(test)]
mod tests {
    use super::safe_join;
    use std::path::Path;

    #[test]
    fn allows_safe_nested_relative_paths() {
        let base = Path::new("/tmp/localsend");
        assert_eq!(
            safe_join(base, "nested/file.txt").unwrap(),
            base.join("nested/file.txt")
        );
        assert_eq!(
            safe_join(base, "a/b/c/file.txt").unwrap(),
            base.join("a/b/c/file.txt")
        );
    }

    #[test]
    fn rejects_symlink_in_nested_path() {
        let dir = std::env::temp_dir().join(format!("lsrs-symlink-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target =
            std::env::temp_dir().join(format!("lsrs-symlink-target-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&target).unwrap();
        let link = dir.join("link_dir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();

        #[cfg(unix)]
        assert!(safe_join(&dir, "link_dir/file.txt").is_err());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn rejects_parent_directory_escape() {
        let base = Path::new("/tmp/localsend");

        assert!(safe_join(base, "../evil.txt").is_err());
        assert!(safe_join(base, "nested/../../evil.txt").is_err());
    }

    #[test]
    fn rejects_absolute_paths() {
        let base = Path::new("/tmp/localsend");

        assert!(safe_join(base, "/tmp/evil.txt").is_err());
    }

    #[test]
    fn rejects_windows_style_paths() {
        let base = Path::new("/tmp/localsend");

        assert!(safe_join(base, "C:\\Users\\evil.txt").is_err());
        assert!(safe_join(base, "nested\\evil.txt").is_err());
    }
}
