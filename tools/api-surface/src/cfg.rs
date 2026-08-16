use std::collections::{BTreeSet, HashMap};

use syn::{Attribute, Expr, Lit, Meta};

use crate::error::ApiSurfaceError;
use crate::model::{Availability, CfgDecision, ScanProfile};

#[derive(Debug, Clone)]
pub struct CfgContext {
    values: HashMap<String, BTreeSet<String>>,
    flags: BTreeSet<String>,
}

impl CfgContext {
    pub fn from_profile(profile: &ScanProfile) -> Self {
        let mut context = Self {
            values: HashMap::new(),
            flags: BTreeSet::new(),
        };
        for feature in &profile.selected_features {
            context.insert_value("feature", feature.clone());
        }
        context.insert_target(&profile.target_triple);
        for value in &profile.cfg {
            context.insert_cfg_value(value);
        }
        context
    }

    pub fn insert_value(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values
            .entry(key.into())
            .or_default()
            .insert(value.into());
    }

    pub fn insert_flag(&mut self, flag: impl Into<String>) {
        self.flags.insert(flag.into());
    }

    fn insert_target(&mut self, target: &str) {
        let triple = target.split('-').collect::<Vec<_>>();
        if let Some(arch) = triple.first() {
            self.insert_value("target_arch", *arch);
        }
        if let Some(os) = triple.get(2) {
            self.insert_value("target_os", *os);
            if *os == "windows" {
                self.insert_flag("windows");
            } else {
                self.insert_flag("unix");
            }
        }
        if let Some(environment) = triple.get(3) {
            self.insert_value("target_env", *environment);
        }
        self.insert_value("target", target);
    }

    fn insert_cfg_value(&mut self, value: &str) {
        if let Some((key, value)) = value.split_once('=') {
            self.insert_value(key.trim(), value.trim_matches('"'));
        } else {
            self.insert_flag(value.trim());
        }
    }

    fn evaluate(&self, meta: &Meta) -> Result<(bool, BTreeSet<String>), ApiSurfaceError> {
        match meta {
            Meta::Path(path) => {
                let key = path.get_ident().map(ToString::to_string).ok_or_else(|| {
                    ApiSurfaceError::configuration("unsupported cfg path", None::<String>)
                })?;
                if self.flags.contains(&key) {
                    return Ok((true, BTreeSet::new()));
                }
                if is_known_key(&key) {
                    return Ok((false, BTreeSet::new()));
                }
                Ok((false, BTreeSet::from([key])))
            }
            Meta::NameValue(name_value) => {
                let key = name_value
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        ApiSurfaceError::configuration("unsupported cfg key", None::<String>)
                    })?;
                let value = match &name_value.value {
                    Expr::Lit(expr) => match &expr.lit {
                        Lit::Str(value) => value.value(),
                        Lit::Int(value) => value.base10_digits().to_owned(),
                        _ => {
                            return Err(ApiSurfaceError::configuration(
                                "cfg values must be strings or integers",
                                None::<String>,
                            ));
                        }
                    },
                    _ => {
                        return Err(ApiSurfaceError::configuration(
                            "cfg values must be literals",
                            None::<String>,
                        ));
                    }
                };
                let active = self
                    .values
                    .get(&key)
                    .is_some_and(|values| values.contains(&value));
                if !is_known_key(&key) {
                    return Ok((active, BTreeSet::from([key])));
                }
                Ok((active, BTreeSet::new()))
            }
            Meta::List(list) => {
                let operator = list
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        ApiSurfaceError::configuration("unsupported cfg operator", None::<String>)
                    })?;
                let nested = list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                    )
                    .map_err(|error| {
                        ApiSurfaceError::configuration(
                            format!("invalid cfg expression: {error}"),
                            None::<String>,
                        )
                    })?;
                let mut decisions = nested
                    .iter()
                    .map(|meta| self.evaluate(meta))
                    .collect::<Result<Vec<_>, _>>()?;
                let unknown = decisions
                    .iter()
                    .flat_map(|(_, unknown)| unknown.iter().cloned())
                    .collect::<BTreeSet<_>>();
                let active = match operator.as_str() {
                    "all" => decisions.iter().all(|(active, _)| *active),
                    "any" => decisions.iter().any(|(active, _)| *active),
                    "not" if decisions.len() == 1 => !decisions.remove(0).0,
                    _ => {
                        return Err(ApiSurfaceError::configuration(
                            format!("unsupported cfg operator `{operator}`"),
                            None::<String>,
                        ));
                    }
                };
                Ok((active, unknown))
            }
        }
    }
}

pub fn availability(
    attributes: &[Attribute],
    context: &CfgContext,
) -> Result<Availability, ApiSurfaceError> {
    let mut result = Availability {
        active: true,
        cfg: Vec::new(),
    };
    for attribute in attributes {
        if !attribute.path().is_ident("cfg") {
            continue;
        }
        let meta = attribute.meta.require_list().map_err(|error| {
            ApiSurfaceError::configuration(
                format!("invalid cfg attribute: {error}"),
                None::<String>,
            )
        })?;
        let expression_meta = syn::parse2::<Meta>(meta.tokens.clone()).map_err(|error| {
            ApiSurfaceError::configuration(
                format!("invalid cfg expression: {error}"),
                None::<String>,
            )
        })?;
        let (active, unknown) = context.evaluate(&expression_meta)?;
        let expression = meta.tokens.to_string();
        if !unknown.is_empty() {
            return Err(ApiSurfaceError::configuration(
                format!("unknown cfg keys in `{expression}`: {unknown:?}"),
                None::<String>,
            ));
        }
        result.active &= active;
        result.cfg.push(CfgDecision {
            expression,
            active,
            unknown,
        });
    }
    Ok(result)
}

fn is_known_key(key: &str) -> bool {
    matches!(
        key,
        "feature"
            | "target"
            | "target_arch"
            | "target_os"
            | "target_env"
            | "target_family"
            | "target_endian"
            | "target_pointer_width"
            | "target_vendor"
            | "target_feature"
            | "unix"
            | "windows"
            | "test"
            | "debug_assertions"
            | "proc_macro"
            | "panic"
    )
}
