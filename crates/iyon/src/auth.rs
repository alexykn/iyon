use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

const PROVIDER: &str = "openai-codex";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPE: &str = "openid profile email offline_access";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";
const KEYRING_SERVICE: &str = "iyon";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CodexCredentials {
    pub access: String,
    pub refresh: String,
    pub expires: u64,
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

pub(crate) fn has_codex_credentials() -> bool {
    load_credentials().is_ok_and(|creds| creds.is_some())
}

/// Returns an OpenRouter API key from `OPENROUTER_API_KEY` (preferred) or the OS keyring.
pub(crate) fn openrouter_api_key() -> Option<String> {
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY")
        && !key.trim().is_empty()
    {
        return Some(key);
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, "openrouter").ok()?;
    entry
        .get_password()
        .ok()
        .filter(|key| !key.trim().is_empty())
}

pub(crate) fn load_credentials() -> Result<Option<CodexCredentials>> {
    for account in [PROVIDER, "openai_codex", "openai-codex-responses"] {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)?;
        let secret = match entry.get_password() {
            Ok(value) => value,
            Err(keyring::Error::NoEntry) => continue,
            Err(err) => return Err(err.into()),
        };
        let creds: CodexCredentials =
            serde_json::from_str(&secret).context("failed to decode stored codex credentials")?;
        return Ok(Some(creds));
    }

    let file = credentials_file_path()?;
    if !file.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&file).context("failed reading local credentials file")?;
    let creds: CodexCredentials =
        serde_json::from_str(&raw).context("failed to decode local codex credentials")?;
    Ok(Some(creds))
}

pub(crate) fn clear_credentials() -> Result<()> {
    for account in [PROVIDER, "openai_codex", "openai-codex-responses"] {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(err) => return Err(err.into()),
        }
    }

    let file = credentials_file_path()?;
    if file.exists() {
        fs::remove_file(file).context("failed to remove local credentials file")?;
    }
    Ok(())
}

pub(crate) async fn get_valid_credentials() -> Result<Option<CodexCredentials>> {
    let Some(creds) = load_credentials()? else {
        return Ok(None);
    };

    let now = now_millis();
    let refresh_skew_ms = 60_000;
    if creds.expires > now.saturating_add(refresh_skew_ms) {
        return Ok(Some(creds));
    }

    let refreshed = refresh_credentials(&creds.refresh).await?;
    persist_credentials(&refreshed)?;
    Ok(Some(refreshed))
}

pub(crate) async fn login() -> Result<()> {
    let verifier = random_urlsafe(32);
    let challenge = pkce_challenge(&verifier);
    let state = random_urlsafe(16);

    let mut url = Url::parse(AUTHORIZE_URL)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "iyon");

    println!("Open this URL to login:\n{url}\n");
    let _ = Command::new("open").arg(url.as_str()).status();

    let code = wait_for_callback_code(&state)?;

    let client = Client::new();
    let token = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", REDIRECT_URI),
        ])
        .send()
        .await
        .context("token exchange request failed")?;

    if !token.status().is_success() {
        let text = token.text().await.unwrap_or_default();
        bail!("token exchange failed: {text}");
    }

    let token: TokenResponse = token.json().await.context("invalid token response")?;
    let account_id = extract_account_id(&token.access_token)
        .context("failed to extract account id from access token")?;
    let expires = now_millis() + token.expires_in.saturating_mul(1000);

    let creds = CodexCredentials {
        access: token.access_token,
        refresh: token
            .refresh_token
            .context("token exchange did not return refresh token")?,
        expires,
        account_id,
    };

    persist_credentials(&creds)?;

    println!("Saved credentials to macOS Keychain and local credentials file.");
    Ok(())
}

async fn refresh_credentials(refresh_token: &str) -> Result<CodexCredentials> {
    let client = Client::new();
    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .context("token refresh request failed")?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        bail!("token refresh failed: {text}");
    }

    let token: TokenResponse = response.json().await.context("invalid refresh response")?;
    let account_id = extract_account_id(&token.access_token)
        .context("failed to extract account id from refreshed access token")?;
    Ok(CodexCredentials {
        access: token.access_token,
        refresh: token
            .refresh_token
            .unwrap_or_else(|| refresh_token.to_string()),
        expires: now_millis() + token.expires_in.saturating_mul(1000),
        account_id,
    })
}

fn persist_credentials(creds: &CodexCredentials) -> Result<()> {
    let serialized = serde_json::to_string(creds)?;

    let entry = keyring::Entry::new(KEYRING_SERVICE, PROVIDER)?;
    entry.set_password(&serialized)?;
    let verify = entry
        .get_password()
        .context("saved credentials but failed to read them back from keychain")?;
    let _: CodexCredentials = serde_json::from_str(&verify)
        .context("saved credentials but failed to parse verification read")?;

    let file = credentials_file_path()?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).context("failed to create credentials dir")?;
    }
    fs::write(&file, serialized).context("failed to write local credentials file")?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))
        .context("failed to set permissions on local credentials file")?;
    Ok(())
}

pub(crate) fn print_status() -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, PROVIDER)?;
    let keychain_ok = entry.get_password().is_ok();
    println!(
        "keychain[{KEYRING_SERVICE}/{PROVIDER}]: {}",
        if keychain_ok { "present" } else { "missing" }
    );

    let file = credentials_file_path()?;
    println!(
        "file[{}]: {}",
        file.display(),
        if file.exists() { "present" } else { "missing" }
    );

    match load_credentials()? {
        Some(creds) => {
            println!("provider: {PROVIDER}");
            println!("account_id: {}", creds.account_id);
            println!("expires_ms: {}", creds.expires);
        }
        None => println!("provider: {PROVIDER} (not logged in)"),
    }
    Ok(())
}

fn wait_for_callback_code(expected_state: &str) -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:1455").context("failed to bind localhost:1455")?;
    listener
        .set_nonblocking(false)
        .context("failed to set listener blocking mode")?;
    listener.set_ttl(64).context("failed to set listener ttl")?;

    let (mut stream, _) = listener
        .accept()
        .context("failed to accept oauth callback")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(180)))
        .context("failed to set callback read timeout")?;

    let mut buf = [0_u8; 8192];
    let n = stream.read(&mut buf).context("failed to read callback")?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or_default();
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let url = Url::parse(&format!("http://localhost{path}"))?;

    if url.path() != "/auth/callback" {
        write_html(&mut stream, 404, "Callback route not found")?;
        bail!("invalid callback path");
    }

    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string());
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string());

    if state.as_deref() != Some(expected_state) {
        write_html(&mut stream, 400, "State mismatch")?;
        bail!("oauth state mismatch");
    }
    let Some(code) = code else {
        write_html(&mut stream, 400, "Missing authorization code")?;
        bail!("missing authorization code");
    };

    write_html(
        &mut stream,
        200,
        "Authentication completed. You can close this window.",
    )?;
    Ok(code)
}

fn write_html(stream: &mut std::net::TcpStream, code: u16, msg: &str) -> Result<()> {
    let body = format!("<html><body><h2>{msg}</h2></body></html>");
    let status = match code {
        200 => "200 OK",
        400 => "400 Bad Request",
        _ => "404 Not Found",
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

fn extract_account_id(jwt: &str) -> Result<String> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        bail!("invalid jwt");
    }
    let payload = URL_SAFE_NO_PAD.decode(parts[1])?;
    let value: serde_json::Value = serde_json::from_slice(&payload)?;
    let account_id = value
        .get(JWT_CLAIM_PATH)
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .context("chatgpt_account_id missing in jwt")?;
    Ok(account_id.to_string())
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn random_urlsafe(len: usize) -> String {
    let mut bytes = vec![0_u8; len];
    rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn credentials_file_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("iyon")
        .join("credentials")
        .join("openai-codex.json"))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
