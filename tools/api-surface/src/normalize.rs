use std::path::Path;

use syn::{Item, Visibility as SynVisibility};

use crate::model::{ApiKind, RustSignature, SourcePosition, SourceSpan, Visibility};

pub fn visibility(visibility: &SynVisibility) -> Visibility {
    match visibility {
        SynVisibility::Public(_) => Visibility::Public,
        SynVisibility::Restricted(restricted) => {
            if restricted.path.is_ident("crate") {
                Visibility::Crate
            } else if restricted.path.is_ident("super") {
                Visibility::Super
            } else {
                Visibility::InPath(normalize_debug(&restricted.path))
            }
        }
        SynVisibility::Inherited => Visibility::Private,
    }
}

pub fn item_kind(item: &Item) -> Option<ApiKind> {
    Some(match item {
        Item::Mod(_) => ApiKind::Module,
        Item::Type(_) => ApiKind::TypeAlias,
        Item::Struct(_) => ApiKind::Struct,
        Item::Enum(_) => ApiKind::Enum,
        Item::Fn(_) => ApiKind::Function,
        Item::Const(_) => ApiKind::Const,
        Item::Static(_) => ApiKind::Static,
        Item::Trait(_) => ApiKind::Trait,
        Item::Impl(_) => ApiKind::Impl,
        Item::Use(_) => return None,
        _ => return None,
    })
}

pub fn signature(item: &Item) -> RustSignature {
    let raw = match item {
        Item::Fn(item) => format!("fn {} {:?}", item.sig.ident, item.sig),
        Item::Struct(item) => format!("struct {} {:?}", item.ident, item.fields),
        Item::Enum(item) => format!("enum {} {:?}", item.ident, item.variants),
        Item::Trait(item) => format!("trait {} {:?}", item.ident, item.items),
        Item::Type(item) => format!("type {} = {:?}", item.ident, item.ty),
        Item::Const(item) => format!("const {}: {:?}", item.ident, item.ty),
        Item::Static(item) => format!("static {}: {:?}", item.ident, item.ty),
        Item::Mod(item) => format!("mod {}", item.ident),
        Item::Impl(item) => format!("impl {:?}", item.self_ty),
        _ => format!("{item:?}"),
    };
    RustSignature(normalize_debug(&raw))
}

pub fn member_signature<T: std::fmt::Debug>(kind: &str, name: &str, value: &T) -> RustSignature {
    RustSignature(normalize_debug(&format!("{kind} {name} {value:?}")))
}

pub fn source_span(path: &Path) -> SourceSpan {
    SourceSpan {
        path: path.to_path_buf(),
        start: SourcePosition { line: 1, column: 0 },
        end: SourcePosition { line: 1, column: 0 },
    }
}

pub fn normalize_debug(value: &impl std::fmt::Debug) -> String {
    normalize_text(&format!("{value:?}"))
}

pub fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
