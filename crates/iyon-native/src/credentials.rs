use keyring::{Entry, Error as KeyringError};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

use crate::NativeError;

fn entry(service: String, account: String) -> Result<Entry> {
    Entry::new(&service, &account)
        .map_err(|error| NativeError::internal(format!("credential store: {error}")))
}

#[napi(js_name = "credentialGet")]
pub fn credential_get(service: String, account: String) -> Result<Option<String>> {
    match entry(service, account)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(NativeError::internal(format!(
            "credential get failed: {error}"
        ))),
    }
}

#[napi(js_name = "credentialSet")]
pub fn credential_set(service: String, account: String, secret: String) -> Result<()> {
    entry(service, account)?
        .set_password(&secret)
        .map_err(|error| NativeError::internal(format!("credential set failed: {error}")))
}

#[napi(js_name = "credentialDelete")]
pub fn credential_delete(service: String, account: String) -> Result<()> {
    match entry(service, account)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(NativeError::internal(format!(
            "credential delete failed: {error}"
        ))),
    }
}

#[napi(js_name = "credentialHas")]
pub fn credential_has(service: String, account: String) -> Result<bool> {
    Ok(credential_get(service, account)?.is_some())
}
