use std::{fs, path::Path};

use anyhow::{Context, bail};

use crate::fs::workspace::Workspace;

const MAX_READ_BYTES: u64 = 1_000_000;

pub fn read_text_file(workspace: &Workspace, path: &str) -> anyhow::Result<String> {
    let resolved = workspace.resolve_safe(path)?;
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("failed to find file: {}", resolved.display()))?;
    workspace.ensure_read_allowed(&canonical)?;

    ensure_regular_file(&canonical)?;
    let metadata = fs::metadata(&canonical)
        .with_context(|| format!("failed to stat file: {}", canonical.display()))?;
    if metadata.len() > MAX_READ_BYTES {
        bail!(
            "file is too large to read ({} bytes, max {} bytes): {}",
            metadata.len(),
            MAX_READ_BYTES,
            canonical.display()
        );
    }

    fs::read_to_string(&canonical)
        .with_context(|| format!("failed to read UTF-8 text file: {}", canonical.display()))
}

fn ensure_regular_file(path: &Path) -> anyhow::Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat path: {}", path.display()))?;
    if metadata.is_dir() {
        bail!("path is a directory: {}", path.display());
    }
    if !metadata.is_file() {
        bail!("path is not a regular file: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use crate::fs::{FsPermissions, Workspace};

    use super::*;

    #[test]
    fn reads_relative_file_under_root() {
        let root = create_temp_dir("read-ok");
        fs::write(root.join("file.txt"), "hello").unwrap();
        let workspace = Workspace::new(root.clone(), FsPermissions::default());

        let text = read_text_file(&workspace, "file.txt").unwrap();

        assert_eq!(text, "hello");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_directory_reads() {
        let root = create_temp_dir("read-dir");
        fs::create_dir(root.join("dir")).unwrap();
        let workspace = Workspace::new(root.clone(), FsPermissions::default());

        let result = read_text_file(&workspace, "dir");

        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_traversal_reads() {
        let root = create_temp_dir("read-traversal-root");
        let outside = root.parent().unwrap().join(unique_name("read-outside"));
        fs::write(&outside, "outside").unwrap();
        let workspace = Workspace::new(root.clone(), FsPermissions::default());

        let result = read_text_file(
            &workspace,
            &format!("../{}", outside.file_name().unwrap().to_string_lossy()),
        );

        assert!(result.is_err());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }

    fn create_temp_dir(prefix: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(unique_name(prefix));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn unique_name(prefix: &str) -> String {
        format!(
            "iyon-{prefix}-{}",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
}
