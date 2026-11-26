//! Unified credential resolution for ArtCraft API access.
//!
//! Priority order:
//! 1. `Authorization: Bearer` on the HTTP transport (remote/companion-brokered).
//!    The bearer gates access; upstream credentials still resolve locally.
//! 2. Environment (in AGENTS.md priority order):
//!    ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN, then ARTCRAFT_API_KEY, then
//!    ARTCRAFT_SESSION + ARTCRAFT_AVT. Note: upstream artcraft_client has no
//!    bearer/API-key auth type — all of these are session-equivalent tokens
//!    and are sent as cookies, matching v1 behavior.
//! 3. Files in ~/Artcraft/credentials/ (artcraft_session.txt, artcraft_avt.txt).
//! 4. The ArtCraft desktop app's .cookies store (platform local data dir).
//!
//! The server starts and serves server/discover and tools/list without any
//! credentials; account-bound calls return a structured auth error instead.

use std::fs;
use std::path::{Path, PathBuf};

use artcraft_client::credentials::storyteller_avt_cookie::StorytellerAvtCookie;
use artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet;
use artcraft_client::credentials::storyteller_session_cookie::StorytellerSessionCookie;
use serde_json::Value;
use tracing::{info, warn};

const CREDENTIALS_DIR: &str = "Artcraft/credentials";
const SESSION_FILE: &str = "artcraft_session.txt";
const AVT_FILE: &str = "artcraft_avt.txt";
const COOKIE_STORE: &str = "ai.artcraft.app/.cookies";

/// Load credentials using the env -> files -> desktop-cookie-store chain.
/// Returns None when nothing is configured; callers must tolerate that.
pub fn load_credentials() -> Option<StorytellerCredentialSet> {
    if let Some(creds) = load_credentials_from_env() {
        info!("Loaded ArtCraft credentials from environment");
        return Some(creds);
    }

    let home_dir = dirs::home_dir()?;
    let cred_dir = home_dir.join(CREDENTIALS_DIR);

    let mut session = load_session_cookie(&cred_dir.join(SESSION_FILE));
    let mut avt = load_avt_cookie(&cred_dir.join(AVT_FILE));

    if session.is_none() || avt.is_none() {
        if let Some(local_app_data) = dirs::data_local_dir() {
            let cookie_store_path = local_app_data.join(COOKIE_STORE);
            if let Some((s, a)) = load_credentials_from_cookie_store(&cookie_store_path) {
                if session.is_none() {
                    session = s;
                }
                if avt.is_none() {
                    avt = a;
                }
            }
        }
    }

    if session.is_none() && avt.is_none() {
        warn!("No ArtCraft credentials found");
        return None;
    }

    info!("Loaded ArtCraft credentials from files/cookie store");
    Some(StorytellerCredentialSet::initialize(avt, session))
}

fn load_credentials_from_env() -> Option<StorytellerCredentialSet> {
    // AGENTS.md credential hierarchy: explicit integration credentials
    // (private MCP token, API key) outrank the raw desktop session cookie.
    let session_val = std::env::var("ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var("ARTCRAFT_API_KEY").ok().filter(|v| !v.trim().is_empty()))
        .or_else(|| std::env::var("ARTCRAFT_SESSION").ok().filter(|v| !v.trim().is_empty()));
    let avt_val = std::env::var("ARTCRAFT_AVT").ok().filter(|v| !v.trim().is_empty());

    if session_val.is_none() && avt_val.is_none() {
        return None;
    }
    Some(StorytellerCredentialSet::initialize(
        avt_val.map(StorytellerAvtCookie::new),
        session_val.map(StorytellerSessionCookie::new),
    ))
}

fn load_session_cookie(path: &Path) -> Option<StorytellerSessionCookie> {
    read_token_file(path).map(StorytellerSessionCookie::new)
}

fn load_avt_cookie(path: &Path) -> Option<StorytellerAvtCookie> {
    read_token_file(path).map(StorytellerAvtCookie::new)
}

fn read_token_file(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(path) {
        Ok(content) => {
            let token = content.trim();
            if token.is_empty() {
                warn!("Credential file is empty: {:?}", path);
                None
            } else {
                Some(token.to_string())
            }
        }
        Err(e) => {
            warn!("Failed to read {:?}: {}", path, e);
            None
        }
    }
}

/// The desktop app stores cookies as a JSON array of objects with a
/// `raw_cookie` string like "session=<token>; ..." / "visitor=<token>; ...".
fn load_credentials_from_cookie_store(
    path: &PathBuf,
) -> Option<(Option<StorytellerSessionCookie>, Option<StorytellerAvtCookie>)> {
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let cookies: Value = serde_json::from_str(&content).ok()?;
    let array = cookies.as_array()?;

    let mut session = None;
    let mut avt = None;

    for cookie in array {
        if let Some(raw) = cookie.get("raw_cookie").and_then(|v| v.as_str()) {
            if raw.starts_with("session=") && session.is_none() {
                let value = raw.split(';').next().unwrap_or(raw);
                let token = value.strip_prefix("session=").unwrap_or(value);
                if !token.is_empty() {
                    session = Some(StorytellerSessionCookie::new(token.to_string()));
                }
            } else if raw.starts_with("visitor=") && avt.is_none() {
                let value = raw.split(';').next().unwrap_or(raw);
                let token = value.strip_prefix("visitor=").unwrap_or(value);
                if !token.is_empty() {
                    avt = Some(StorytellerAvtCookie::new(token.to_string()));
                }
            }
        }
    }

    if session.is_none() && avt.is_none() {
        return None;
    }
    Some((session, avt))
}

/// Access gate for the HTTP transport. When `ARTCRAFT_MCP_BEARER_TOKEN` (or the
/// companion-brokered token) is configured, presented bearer must match it.
/// When nothing is configured, loopback access is allowed without a token and
/// remote binds must pass `--allow-remote` (enforced in main).
pub fn bearer_access_allowed(configured: Option<&str>, presented: Option<&str>) -> bool {
    match configured {
        None => true,
        Some(expected) => presented == Some(expected),
    }
}
