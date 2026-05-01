#![allow(dead_code)]

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};

use crate::fs::permissions::FsPermissions;

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    permissions: FsPermissions,
}

impl Workspace {
    pub fn new(root: PathBuf, permissions: FsPermissions) -> Self {
        let root = root
            .canonicalize()
            .unwrap_or_else(|_| absolutize_lexical(&root));
        Self { root, permissions }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve_safe(&self, path: &str) -> anyhow::Result<PathBuf> {
        self.resolve_path_for_operation(path, true, true)
    }

    pub fn resolve_read_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        let resolved = self.resolve_path_for_operation(path, true, false)?;
        self.ensure_read_allowed(&resolved)?;
        Ok(resolved)
    }

    pub fn resolve_write_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        self.resolve_path_for_operation(path, false, true)
    }

    pub fn resolve_search_path(&self, path: Option<&str>) -> anyhow::Result<PathBuf> {
        let path = path.unwrap_or(".");
        let resolved = self.resolve_path_for_operation(path, true, false)?;
        self.ensure_read_allowed(&resolved)?;
        Ok(resolved)
    }

    fn resolve_path_for_operation(
        &self,
        path: &str,
        read: bool,
        write: bool,
    ) -> anyhow::Result<PathBuf> {
        if path.trim().is_empty() {
            bail!("path must not be empty");
        }

        let expanded = expand_home(path);
        if !self.permissions.allow_hidden && contains_hidden_component(Path::new(&expanded)) {
            bail!("hidden paths are not allowed: {path}");
        }

        for pattern in &self.permissions.deny_patterns {
            if !pattern.is_empty() && expanded.contains(pattern) {
                bail!("path matches denied pattern: {pattern}");
            }
        }

        let input = Path::new(&expanded);
        let joined = if input.is_absolute() {
            input.to_path_buf()
        } else {
            self.root.join(input)
        };
        let resolved = normalize_lexical(&joined);

        let allow_outside_root = (read && self.permissions.allow_read_outside_root)
            || (write && self.permissions.allow_write_outside_root);
        if !resolved.starts_with(&self.root) && !allow_outside_root {
            bail!("path escapes workspace root: {path}");
        }

        Ok(resolved)
    }

    pub fn ensure_read_allowed(&self, path: &Path) -> anyhow::Result<()> {
        self.ensure_path_allowed(path, self.permissions.allow_read_outside_root, "read")
    }

    pub fn ensure_write_allowed(&self, path: &Path) -> anyhow::Result<()> {
        self.ensure_path_allowed(path, self.permissions.allow_write_outside_root, "write")
    }

    fn ensure_path_allowed(
        &self,
        path: &Path,
        allow_outside_root: bool,
        operation: &str,
    ) -> anyhow::Result<()> {
        let path = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize path: {}", path.display()))?;

        if !allow_outside_root && !path.starts_with(&self.root) {
            bail!(
                "{operation} outside workspace root is not allowed: {}",
                path.display()
            );
        }

        if !self.permissions.allow_hidden && path_has_hidden_relative_component(&self.root, &path) {
            bail!("hidden paths are not allowed: {}", path.display());
        }

        let path_text = path.to_string_lossy();
        for pattern in &self.permissions.deny_patterns {
            if !pattern.is_empty() && path_text.contains(pattern) {
                bail!("path matches denied pattern: {pattern}");
            }
        }

        Ok(())
    }
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return home_dir().map_or_else(
            || path.to_string(),
            |home| home.to_string_lossy().into_owned(),
        );
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir().map_or_else(
            || path.to_string(),
            |home| home.join(rest).to_string_lossy().into_owned(),
        );
    }
    path.to_string()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn absolutize_lexical(path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_lexical(path)
    } else {
        normalize_lexical(
            &std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path),
        )
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

fn contains_hidden_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(part) => part.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

fn path_has_hidden_relative_component(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    contains_hidden_component(relative)
}
