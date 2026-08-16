use std::path::{Path, PathBuf};

use syn::{Attribute, Item, ItemMod};

use crate::cfg::{CfgContext, availability};
use crate::error::ApiSurfaceError;
use crate::model::{Availability, ScanProfile};

#[derive(Debug)]
pub struct ParsedItem {
    pub item: Item,
    pub availability: Availability,
}

#[derive(Debug)]
pub struct ModuleNode {
    pub path: Vec<String>,
    pub source_path: PathBuf,
    pub items: Vec<ParsedItem>,
    pub children: Vec<ModuleNode>,
}

#[derive(Debug)]
pub struct ParseDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug)]
pub struct ModuleTree {
    pub root: ModuleNode,
    pub diagnostics: Vec<ParseDiagnostic>,
}

pub struct SourceLoader {
    context: CfgContext,
    diagnostics: Vec<ParseDiagnostic>,
}

impl SourceLoader {
    pub fn new(profile: &ScanProfile) -> Self {
        Self {
            context: CfgContext::from_profile(profile),
            diagnostics: Vec::new(),
        }
    }

    pub fn load(mut self, source_root: impl AsRef<Path>) -> Result<ModuleTree, ApiSurfaceError> {
        let source_root = source_root.as_ref().to_path_buf();
        let root = self.load_file(Vec::new(), source_root)?;
        Ok(ModuleTree {
            root,
            diagnostics: self.diagnostics,
        })
    }

    fn load_file(
        &mut self,
        path: Vec<String>,
        source_path: PathBuf,
    ) -> Result<ModuleNode, ApiSurfaceError> {
        let source =
            std::fs::read_to_string(&source_path).map_err(|error| ApiSurfaceError::Source {
                path: source_path.display().to_string(),
                item: None,
                message: error.to_string(),
            })?;
        let file = syn::parse_file(&source).map_err(|error| ApiSurfaceError::Source {
            path: source_path.display().to_string(),
            item: None,
            message: format!("Rust parse failed: {error}"),
        })?;
        let mut items = Vec::new();
        let mut children = Vec::new();
        for item in file.items {
            let item_availability = availability(attrs(&item), &self.context)?;
            if let Item::Mod(module) = &item {
                if let Some(child) =
                    self.load_child(&path, &source_path, module, item_availability.active)?
                {
                    children.push(child);
                }
            }
            items.push(ParsedItem {
                item,
                availability: item_availability,
            });
        }
        Ok(ModuleNode {
            path,
            source_path,
            items,
            children,
        })
    }

    fn load_child(
        &mut self,
        parent_path: &[String],
        parent_file: &Path,
        module: &ItemMod,
        active: bool,
    ) -> Result<Option<ModuleNode>, ApiSurfaceError> {
        let module_name = module.ident.to_string();
        let mut child_path = parent_path.to_vec();
        child_path.push(module_name.clone());
        let Some((_, inline_items)) = &module.content else {
            let candidates = external_module_candidates(parent_file, &module_name);
            let existing = candidates
                .iter()
                .filter(|candidate| candidate.exists())
                .collect::<Vec<_>>();
            if existing.len() > 1 {
                return Err(ApiSurfaceError::Source {
                    path: parent_file.display().to_string(),
                    item: Some(child_path.join("::")),
                    message: format!("ambiguous module files: {existing:?}"),
                });
            }
            let Some(path) = existing.first() else {
                self.diagnostics.push(ParseDiagnostic {
                    path: parent_file.to_path_buf(),
                    message: format!("module `{module_name}` has no source file"),
                });
                return Ok(None);
            };
            if !active {
                self.diagnostics.push(ParseDiagnostic {
                    path: (*path).to_path_buf(),
                    message: format!(
                        "inactive module `{}` retained as metadata",
                        child_path.join("::")
                    ),
                });
            }
            return self.load_file(child_path, (*path).to_path_buf()).map(Some);
        };
        if !active {
            self.diagnostics.push(ParseDiagnostic {
                path: parent_file.to_path_buf(),
                message: format!(
                    "inactive inline module `{}` retained as metadata",
                    child_path.join("::")
                ),
            });
        }
        let mut items = Vec::new();
        let mut children = Vec::new();
        for item in inline_items.clone() {
            let item_availability = availability(attrs(&item), &self.context)?;
            if let Item::Mod(child_module) = &item {
                if let Some(child) = self.load_child(
                    &child_path,
                    parent_file,
                    child_module,
                    item_availability.active,
                )? {
                    children.push(child);
                }
            }
            items.push(ParsedItem {
                item,
                availability: item_availability,
            });
        }
        Ok(Some(ModuleNode {
            path: child_path,
            source_path: parent_file.to_path_buf(),
            items,
            children,
        }))
    }
}

fn attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Verbatim(_) => &[],
        _ => &[],
    }
}

fn external_module_candidates(parent_file: &Path, module_name: &str) -> [PathBuf; 2] {
    let directory = parent_file.parent().unwrap_or_else(|| Path::new("."));
    [
        directory.join(format!("{module_name}.rs")),
        directory.join(module_name).join("mod.rs"),
    ]
}
