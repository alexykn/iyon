#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct FsPermissions {
    pub allow_read_outside_root: bool,
    pub allow_write_outside_root: bool,
    pub deny_patterns: Vec<String>,
    pub allow_hidden: bool,
}

impl Default for FsPermissions {
    fn default() -> Self {
        Self {
            allow_read_outside_root: false,
            allow_write_outside_root: false,
            deny_patterns: Vec::new(),
            allow_hidden: true,
        }
    }
}
