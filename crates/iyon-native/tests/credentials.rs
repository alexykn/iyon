use iyon_native::{credential_delete, credential_get, credential_has, credential_set};

#[test]
fn credential_boundary_accepts_only_owned_opaque_values() {
    let _get: fn(String, String) -> napi::bindgen_prelude::Result<Option<String>> = credential_get;
    let _set: fn(String, String, String) -> napi::bindgen_prelude::Result<()> = credential_set;
    let _delete: fn(String, String) -> napi::bindgen_prelude::Result<()> = credential_delete;
    let _has: fn(String, String) -> napi::bindgen_prelude::Result<bool> = credential_has;
}
