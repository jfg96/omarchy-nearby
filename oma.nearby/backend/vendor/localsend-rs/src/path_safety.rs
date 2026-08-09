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
    let mut normal_components = 0usize;
    for component in remote_path.components() {
        match component {
            Component::Normal(part) => {
                normal_components += 1;
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

    // Remote file names are display names, not directory instructions. Keeping
    // them to one component also removes symlinked-parent and TOCTOU escapes.
    if normal_components != 1 {
        return Err(LocalSendError::invalid_file(format!(
            "Unsafe nested remote file name: {}",
            remote_name
        )));
    }

    Ok(base.join(relative))
}

#[cfg(test)]
mod tests {
    use super::safe_join;
    use std::path::Path;

    #[test]
    fn rejects_nested_relative_paths_to_prevent_symlink_parent_escape() {
        let base = Path::new("/tmp/localsend");
        assert!(safe_join(base, "nested/file.txt").is_err());
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
