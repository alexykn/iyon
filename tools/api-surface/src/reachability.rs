use std::collections::{BTreeMap, BTreeSet, VecDeque};

use syn::{
    Fields, ImplItem, Item, ItemTrait, TraitItem, Type, TypePath, UseTree,
    Visibility as SynVisibility,
};

use crate::error::ApiSurfaceError;
use crate::model::{
    ApiItemId, ApiKind, ApiPath, Availability, CrateId, ReachabilityTrace, ReachableSurface,
    SurfaceItem, SurfacePath, Visibility,
};
use crate::normalize::{item_kind, member_signature, signature, source_span, visibility};
use crate::parse::{ModuleNode, ParsedItem};

#[derive(Debug, Clone)]
struct Declaration {
    path: Vec<String>,
    kind: ApiKind,
    item: Item,
    visibility: Visibility,
    source: std::path::PathBuf,
    availability: Availability,
}

#[derive(Debug, Clone)]
struct UseBinding {
    module: Vec<String>,
    local: Vec<String>,
    raw_target: Vec<String>,
    visibility: Visibility,
    source: std::path::PathBuf,
    glob: bool,
    availability: Availability,
}

#[derive(Debug, Default)]
struct Namespace {
    modules: BTreeSet<Vec<String>>,
    declarations: BTreeMap<Vec<String>, Declaration>,
    uses: Vec<UseBinding>,
    impls: Vec<Declaration>,
}

pub fn resolve(
    crate_id: impl Into<String>,
    root: &ModuleNode,
) -> Result<ReachableSurface, ApiSurfaceError> {
    let crate_id = CrateId(crate_id.into());
    let mut namespace = Namespace::default();
    collect_module(root, &mut namespace)?;

    let mut resolver = Resolver {
        crate_id: crate_id.clone(),
        namespace,
        items: BTreeMap::new(),
        paths: BTreeMap::new(),
        accessible_modules: BTreeSet::from([Vec::new()]),
        queue: VecDeque::from([Vec::new()]),
    };
    resolver.discover_public_exports();
    resolver.discover_members();

    let items = resolver.items.into_values().collect::<Vec<_>>();
    let paths = resolver.paths.into_values().collect::<Vec<_>>();
    Ok(ReachableSurface {
        crate_id,
        items,
        paths,
    })
}

struct Resolver {
    crate_id: CrateId,
    namespace: Namespace,
    items: BTreeMap<String, SurfaceItem>,
    paths: BTreeMap<String, SurfacePath>,
    accessible_modules: BTreeSet<Vec<String>>,
    queue: VecDeque<Vec<String>>,
}

impl Resolver {
    fn discover_public_exports(&mut self) {
        while let Some(module) = self.queue.pop_front() {
            let direct = self
                .namespace
                .declarations
                .values()
                .filter(|declaration| {
                    parent_path(&declaration.path) == module
                        && declaration.visibility == Visibility::Public
                        && declaration.availability.active
                })
                .cloned()
                .collect::<Vec<_>>();
            for declaration in direct {
                self.add_declaration(
                    &declaration,
                    declaration.path.clone(),
                    false,
                    trace(format!("public declaration in {}", display_module(&module))),
                );
                if declaration.kind == ApiKind::Module {
                    self.add_module(declaration.path.clone());
                }
            }

            let bindings = self
                .namespace
                .uses
                .iter()
                .filter(|binding| {
                    binding.module == module
                        && binding.visibility == Visibility::Public
                        && binding.availability.active
                })
                .cloned()
                .collect::<Vec<_>>();
            for binding in bindings {
                if binding.glob {
                    let Some(target_module) =
                        self.resolve_path(&binding.module, &binding.raw_target)
                    else {
                        continue;
                    };
                    for (path, declaration) in self.public_module_exports(&target_module) {
                        let mut public_path = binding.module.clone();
                        public_path.extend(
                            path.strip_prefix(target_module.as_slice())
                                .unwrap_or(&[])
                                .iter()
                                .cloned(),
                        );
                        self.add_declaration(
                            &declaration,
                            public_path,
                            true,
                            trace(format!(
                                "glob re-export from {} at {}",
                                join_path(&target_module),
                                binding.source.display()
                            )),
                        );
                    }
                    continue;
                }
                let Some(canonical) = self.resolve_path(&binding.module, &binding.raw_target)
                else {
                    continue;
                };
                let mut public_path = binding.module.clone();
                public_path.extend(binding.local.iter().skip(binding.module.len()).cloned());
                let Some(declaration) = self.namespace.declarations.get(&canonical).cloned() else {
                    continue;
                };
                self.add_declaration(
                    &declaration,
                    public_path,
                    true,
                    trace(format!(
                        "pub use from {} at {}",
                        join_path(&canonical),
                        binding.source.display()
                    )),
                );
            }
        }
    }

    fn public_module_exports(&self, module: &[String]) -> Vec<(Vec<String>, Declaration)> {
        let mut exports = self
            .namespace
            .declarations
            .values()
            .filter(|declaration| {
                parent_path(&declaration.path) == module
                    && declaration.visibility == Visibility::Public
                    && declaration.availability.active
            })
            .map(|declaration| (declaration.path.clone(), declaration.clone()))
            .collect::<Vec<_>>();
        for binding in self.namespace.uses.iter().filter(|binding| {
            binding.module == module
                && binding.visibility == Visibility::Public
                && binding.availability.active
        }) {
            if binding.glob {
                continue;
            }
            if let Some(canonical) = self.resolve_path(&binding.module, &binding.raw_target) {
                if let Some(declaration) = self.namespace.declarations.get(&canonical) {
                    exports.push((binding.local.clone(), declaration.clone()));
                }
            }
        }
        exports
    }

    fn discover_members(&mut self) {
        let reachable = self
            .items
            .values()
            .map(|item| item.canonical_path.segments.clone())
            .collect::<BTreeSet<_>>();
        let declarations = self
            .namespace
            .declarations
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for declaration in declarations {
            match &declaration.item {
                Item::Struct(item) if reachable.contains(&declaration.path) => {
                    self.add_struct_members(&declaration, &item.fields);
                }
                Item::Enum(item) if reachable.contains(&declaration.path) => {
                    for variant in &item.variants {
                        let mut path = declaration.path.clone();
                        path.push(variant.ident.to_string());
                        let variant_id = self.add_synthetic(
                            &path,
                            ApiKind::Variant,
                            member_signature("variant", &variant.ident.to_string(), variant),
                            &declaration,
                            path.clone(),
                            trace("enum variant"),
                        );
                        self.add_variant_fields(&path, &variant.fields, &declaration, variant_id);
                    }
                }
                Item::Trait(item) if reachable.contains(&declaration.path) => {
                    self.add_trait_members(&declaration, item);
                }
                _ => {}
            }
        }
        self.add_impl_members(&reachable);
    }

    fn add_struct_members(&mut self, declaration: &Declaration, fields: &Fields) {
        for (index, field) in fields.iter().enumerate() {
            if visibility(&field.vis) != Visibility::Public {
                continue;
            }
            let Some(name) = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .or_else(|| Some(index.to_string()))
            else {
                continue;
            };
            let mut path = declaration.path.clone();
            path.push(name);
            self.add_synthetic(
                &path,
                ApiKind::StructField,
                member_signature("field", path.last().unwrap_or(&String::new()), field),
                declaration,
                path.clone(),
                trace("public struct field"),
            );
        }
    }

    fn add_variant_fields(
        &mut self,
        variant_path: &[String],
        fields: &Fields,
        declaration: &Declaration,
        _variant_id: String,
    ) {
        for (index, field) in fields.iter().enumerate() {
            let Some(name) = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .or_else(|| Some(index.to_string()))
            else {
                continue;
            };
            let mut path = variant_path.to_vec();
            path.push(name);
            self.add_synthetic(
                &path,
                ApiKind::VariantField,
                member_signature(
                    "variant-field",
                    path.last().unwrap_or(&String::new()),
                    field,
                ),
                declaration,
                path.clone(),
                trace("enum variant field"),
            );
        }
    }

    fn add_trait_members(&mut self, declaration: &Declaration, item: &ItemTrait) {
        for trait_item in &item.items {
            let (name, kind, signature) = match trait_item {
                TraitItem::Fn(item) => (
                    item.sig.ident.to_string(),
                    if item.sig.receiver().is_some() {
                        ApiKind::Method
                    } else {
                        ApiKind::AssociatedFunction
                    },
                    member_signature("trait-fn", &item.sig.ident.to_string(), &item.sig),
                ),
                TraitItem::Const(item) => (
                    item.ident.to_string(),
                    ApiKind::AssociatedConst,
                    member_signature("trait-const", &item.ident.to_string(), item),
                ),
                TraitItem::Type(item) => (
                    item.ident.to_string(),
                    ApiKind::AssociatedType,
                    member_signature("trait-type", &item.ident.to_string(), item),
                ),
                _ => continue,
            };
            let mut path = declaration.path.clone();
            path.push(name);
            self.add_synthetic(
                &path,
                kind,
                signature,
                declaration,
                path.clone(),
                trace("trait associated item"),
            );
        }
    }

    fn add_impl_members(&mut self, reachable: &BTreeSet<Vec<String>>) {
        let impls = self.namespace.impls.clone();
        for declaration in impls {
            let Item::Impl(item) = &declaration.item else {
                continue;
            };
            let Some(receiver) = simple_type_path(&item.self_ty) else {
                continue;
            };
            let Some(receiver) = self.resolve_path(&declaration.path, &receiver) else {
                continue;
            };
            if !reachable.contains(&receiver) {
                continue;
            }
            let trait_path = item.trait_.as_ref().map(|(_, path, _)| {
                path.segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
            });
            if let Some(ref trait_path) = trait_path {
                let projection = format!("{} as {}", join_path(&receiver), join_path(&trait_path));
                self.add_synthetic(
                    &receiver,
                    ApiKind::TraitProjection,
                    crate::model::RustSignature(projection),
                    &declaration,
                    receiver.clone(),
                    trace("trait implementation projection"),
                );
            }
            for impl_item in &item.items {
                let (name, kind, sig, public) = match impl_item {
                    ImplItem::Fn(item) => (
                        item.sig.ident.to_string(),
                        if item.sig.receiver().is_some() {
                            ApiKind::Method
                        } else {
                            ApiKind::AssociatedFunction
                        },
                        member_signature("impl-fn", &item.sig.ident.to_string(), &item.sig),
                        matches!(item.vis, SynVisibility::Public(_)),
                    ),
                    ImplItem::Const(item) => (
                        item.ident.to_string(),
                        ApiKind::AssociatedConst,
                        member_signature("impl-const", &item.ident.to_string(), item),
                        true,
                    ),
                    ImplItem::Type(item) => (
                        item.ident.to_string(),
                        ApiKind::AssociatedType,
                        member_signature("impl-type", &item.ident.to_string(), item),
                        true,
                    ),
                    _ => continue,
                };
                if !public && trait_path.is_none() {
                    continue;
                }
                let mut path = receiver.clone();
                path.push(name);
                self.add_synthetic(
                    &path,
                    kind,
                    sig,
                    &declaration,
                    path.clone(),
                    trace("impl associated item"),
                );
            }
        }
    }

    fn add_module(&mut self, module: Vec<String>) {
        if self.accessible_modules.insert(module.clone()) {
            self.queue.push_back(module);
        }
    }

    fn add_declaration(
        &mut self,
        declaration: &Declaration,
        public_path: Vec<String>,
        alias: bool,
        trace: ReachabilityTrace,
    ) {
        self.add_surface_item(
            declaration.path.clone(),
            declaration.kind.clone(),
            signature(&declaration.item),
            declaration.visibility.clone(),
            declaration.source.clone(),
            declaration.availability.clone(),
            public_path,
            alias,
            trace,
        );
    }

    fn add_synthetic(
        &mut self,
        path: &[String],
        kind: ApiKind,
        sig: crate::model::RustSignature,
        source_decl: &Declaration,
        public_path: Vec<String>,
        trace: ReachabilityTrace,
    ) -> String {
        self.add_surface_item(
            path.to_vec(),
            kind,
            sig,
            Visibility::Public,
            source_decl.source.clone(),
            source_decl.availability.clone(),
            public_path,
            false,
            trace,
        )
    }

    fn add_surface_item(
        &mut self,
        canonical: Vec<String>,
        kind: ApiKind,
        sig: crate::model::RustSignature,
        visibility: Visibility,
        source: std::path::PathBuf,
        availability: Availability,
        public_path: Vec<String>,
        alias: bool,
        trace: ReachabilityTrace,
    ) -> String {
        let id = item_id(&self.crate_id, &canonical);
        let path = api_path(&self.crate_id, &public_path);
        let surface_path = SurfacePath {
            path: path.clone(),
            alias,
            trace: trace.clone(),
        };
        self.paths
            .entry(path.display())
            .or_insert_with(|| surface_path.clone());
        self.items
            .entry(id.clone())
            .and_modify(|item| {
                if !item.paths.iter().any(|existing| existing.path == path) {
                    item.paths.push(surface_path.clone());
                    item.paths.sort();
                }
            })
            .or_insert_with(|| SurfaceItem {
                id: ApiItemId(id.clone()),
                canonical_path: api_path(&self.crate_id, &canonical),
                kind,
                signature: sig,
                visibility,
                source: source_span(&source),
                availability,
                paths: vec![surface_path],
            });
        id
    }

    fn resolve_path(&self, module: &[String], raw: &[String]) -> Option<Vec<String>> {
        let mut base = if raw.first().is_some_and(|segment| segment == "crate") {
            Vec::new()
        } else {
            module.to_vec()
        };
        let mut parts = raw.iter();
        if raw.first().is_some_and(|segment| segment == "crate") {
            parts.next();
        }
        while let Some(segment) = parts.next() {
            match segment.as_str() {
                "self" => {}
                "super" => {
                    base.pop();
                }
                segment => base.push(segment.to_owned()),
            }
        }
        self.resolve_absolute(&base, &mut BTreeSet::new())
    }

    fn resolve_absolute(
        &self,
        path: &[String],
        seen: &mut BTreeSet<Vec<String>>,
    ) -> Option<Vec<String>> {
        if self.namespace.declarations.contains_key(path) || self.namespace.modules.contains(path) {
            return Some(path.to_vec());
        }
        if !seen.insert(path.to_vec()) {
            return None;
        }
        for binding in &self.namespace.uses {
            if binding.glob || binding.local != path {
                continue;
            }
            if let Some(resolved) = self.resolve_path(&binding.module, &binding.raw_target) {
                return self.resolve_absolute(&resolved, seen).or(Some(resolved));
            }
        }
        None
    }
}

fn collect_module(module: &ModuleNode, namespace: &mut Namespace) -> Result<(), ApiSurfaceError> {
    namespace.modules.insert(module.path.clone());
    for parsed in &module.items {
        collect_item(&module.path, parsed, &module.source_path, namespace)?;
    }
    for child in &module.children {
        collect_module(child, namespace)?;
    }
    Ok(())
}

fn collect_item(
    module: &[String],
    parsed: &ParsedItem,
    source: &std::path::Path,
    namespace: &mut Namespace,
) -> Result<(), ApiSurfaceError> {
    match &parsed.item {
        Item::Use(item) => {
            let mut bindings = Vec::new();
            flatten_use_tree(&item.tree, Vec::new(), &mut bindings);
            for (raw_target, local, glob) in bindings {
                let mut local_path = module.to_vec();
                local_path.extend(local);
                namespace.uses.push(UseBinding {
                    module: module.to_vec(),
                    local: local_path,
                    raw_target,
                    visibility: visibility(&item.vis),
                    source: source.to_path_buf(),
                    glob,
                    availability: parsed.availability.clone(),
                });
            }
        }
        Item::Impl(_) => {
            namespace.impls.push(Declaration {
                path: module.to_vec(),
                kind: ApiKind::Impl,
                item: parsed.item.clone(),
                visibility: Visibility::Public,
                source: source.to_path_buf(),
                availability: parsed.availability.clone(),
            });
        }
        item => {
            let Some(kind) = item_kind(item) else {
                return Ok(());
            };
            let Some(name) = item_name(item) else {
                return Ok(());
            };
            let mut path = module.to_vec();
            path.push(name);
            let item_visibility = item_visibility(item);
            namespace.declarations.insert(
                path.clone(),
                Declaration {
                    path,
                    kind,
                    item: item.clone(),
                    visibility: item_visibility,
                    source: source.to_path_buf(),
                    availability: parsed.availability.clone(),
                },
            );
        }
    }
    Ok(())
}

fn flatten_use_tree(
    tree: &UseTree,
    prefix: Vec<String>,
    result: &mut Vec<(Vec<String>, Vec<String>, bool)>,
) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(path.ident.to_string());
            flatten_use_tree(&path.tree, prefix, result);
        }
        UseTree::Name(name) => {
            let mut raw = prefix.clone();
            raw.push(name.ident.to_string());
            result.push((raw, vec![name.ident.to_string()], false));
        }
        UseTree::Rename(rename) => {
            let mut raw = prefix;
            raw.push(rename.ident.to_string());
            result.push((raw, vec![rename.rename.to_string()], false));
        }
        UseTree::Glob(_) => result.push((prefix, Vec::new(), true)),
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(item, prefix.clone(), result);
            }
        }
    }
}

fn item_name(item: &Item) -> Option<String> {
    Some(match item {
        Item::Mod(item) => item.ident.to_string(),
        Item::Type(item) => item.ident.to_string(),
        Item::Struct(item) => item.ident.to_string(),
        Item::Enum(item) => item.ident.to_string(),
        Item::Fn(item) => item.sig.ident.to_string(),
        Item::Const(item) => item.ident.to_string(),
        Item::Static(item) => item.ident.to_string(),
        Item::Trait(item) => item.ident.to_string(),
        _ => return None,
    })
}

fn item_visibility(item: &Item) -> Visibility {
    match item {
        Item::Mod(item) => visibility(&item.vis),
        Item::Type(item) => visibility(&item.vis),
        Item::Struct(item) => visibility(&item.vis),
        Item::Enum(item) => visibility(&item.vis),
        Item::Fn(item) => visibility(&item.vis),
        Item::Const(item) => visibility(&item.vis),
        Item::Static(item) => visibility(&item.vis),
        Item::Trait(item) => visibility(&item.vis),
        _ => Visibility::Private,
    }
}

fn simple_type_path(ty: &Type) -> Option<Vec<String>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return None;
    };
    Some(
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect(),
    )
}

fn parent_path(path: &[String]) -> Vec<String> {
    path.get(..path.len().saturating_sub(1))
        .unwrap_or_default()
        .to_vec()
}
fn display_module(path: &[String]) -> String {
    if path.is_empty() {
        "crate".into()
    } else {
        join_path(path)
    }
}
fn join_path(path: &[String]) -> String {
    path.join("::")
}
fn item_id(crate_id: &CrateId, path: &[String]) -> String {
    format!("{}::{}", crate_id.0, join_path(path))
}
fn api_path(crate_id: &CrateId, path: &[String]) -> ApiPath {
    ApiPath::new(crate_id.0.clone(), path.to_vec())
}
fn trace(step: impl Into<String>) -> ReachabilityTrace {
    ReachabilityTrace {
        steps: vec![step.into()],
    }
}
