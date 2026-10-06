use std::fs;
use std::path::PathBuf;
use anyhow::{anyhow, Result};
use log::{info, warn};
use directories::UserDirs;

use artcraft_client::credentials::storyteller_avt_cookie::StorytellerAvtCookie;
use artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet;
use artcraft_client::credentials::storyteller_session_cookie::StorytellerSessionCookie;
use artcraft_client::endpoints::mcp_sessions::create_mcp_session::create_mcp_session;
use artcraft_client::utils::api_host::ApiHost;
use artcraft_api_defs::mcp_sessions::create_mcp_session::CreateMcpSessionRequest;
use tokens::tokens::mcp_session_private::McpSessionPrivateToken;

const MCP_CLIENT_NAME: &str = "artcraft-mcp-server";

pub async fn resolve_credentials(api_host: &ApiHost) -> Result<StorytellerCredentialSet> {
  // 1. Direct MCP private session token (preferred)
  if let Ok(token) = std::env::var("ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN") {
    info!("Resolving credentials from ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN");
    return Ok(StorytellerCredentialSet::initialize_with_mcp_token(
      McpSessionPrivateToken::new_from_str(&token),
    ));
  }

  // 2. API key -> exchange for an MCP session
  if let Ok(api_key) = std::env::var("ARTCRAFT_API_KEY") {
    info!("Resolving credentials from ARTCRAFT_API_KEY; creating MCP session");
    let initial_creds = StorytellerCredentialSet::initialize_with_api_key(api_key);
    return create_mcp_session_from_creds(api_host, &initial_creds).await;
  }

  // 3. Session/AVT cookies from environment
  let env_session = std::env::var("ARTCRAFT_SESSION").ok();
  let env_avt = std::env::var("ARTCRAFT_AVT").ok();

  if env_session.is_some() || env_avt.is_some() {
    info!("Resolving credentials from ARTCRAFT_SESSION / ARTCRAFT_AVT");
    let session = env_session.map(StorytellerSessionCookie::new);
    let avt = env_avt.map(StorytellerAvtCookie::new);
    let initial_creds = StorytellerCredentialSet::initialize(avt, session);
    return create_mcp_session_from_creds(api_host, &initial_creds).await;
  }

  // 4. Standard desktop credential directory
  if let Some(user_dirs) = UserDirs::new() {
    let home = user_dirs.home_dir();
    let artcraft_dir = home.join("Artcraft");
    let creds_dir = artcraft_dir.join("credentials");

    let session_path = creds_dir.join("artcraft_session.txt");
    let avt_path = creds_dir.join("artcraft_avt.txt");
    let api_key_path = creds_dir.join("artcraft_api_key.txt");

    if let Some(api_key) = read_trimmed_file(&api_key_path) {
      info!("Resolving API key from desktop credentials directory");
      let initial_creds = StorytellerCredentialSet::initialize_with_api_key(api_key);
      return create_mcp_session_from_creds(api_host, &initial_creds).await;
    }

    let session = read_trimmed_file(&session_path).map(StorytellerSessionCookie::new);
    let avt = read_trimmed_file(&avt_path).map(StorytellerAvtCookie::new);

    if session.is_some() || avt.is_some() {
      info!("Resolving session cookies from desktop credentials directory");
      let initial_creds = StorytellerCredentialSet::initialize(avt, session);
      return create_mcp_session_from_creds(api_host, &initial_creds).await;
    }
  }

  // 5. Fallback to local artcraft_cookies.txt
  let local_cookie_path = PathBuf::from("artcraft_cookies.txt");
  if local_cookie_path.exists() {
    info!("Attempting to read credentials from local cookie file");
    if let Ok(contents) = fs::read_to_string(&local_cookie_path) {
      if let Ok(Some(creds)) = StorytellerCredentialSet::parse_multi_cookie_header(contents.trim()) {
        return create_mcp_session_from_creds(api_host, &creds).await;
      }
    }
  }

  warn!("No credentials found. Requests requiring auth will fail.");
  Err(anyhow!(
    "Could not resolve credentials. Please set ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN, \
     ARTCRAFT_API_KEY, or ARTCRAFT_SESSION and ARTCRAFT_AVT environment variables, \
     ensure the desktop app is logged in, or place artcraft_cookies.txt in the current directory."
  ))
}

async fn create_mcp_session_from_creds(
  api_host: &ApiHost,
  initial_creds: &StorytellerCredentialSet,
) -> Result<StorytellerCredentialSet> {
  let request = CreateMcpSessionRequest {
    maybe_mcp_client_name: Some(MCP_CLIENT_NAME.to_string()),
    maybe_mcp_client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
    maybe_mcp_client_vendor: None,
  };

  let response = create_mcp_session(api_host, Some(initial_creds), request)
    .await
    .map_err(|e| anyhow!("Failed to create MCP session: {:?}", e))?;

  info!("Created MCP session from existing credentials");

  Ok(StorytellerCredentialSet::initialize_with_mcp_tokens(
    response.private_session_token,
    response.private_refresh_token,
  ))
}

fn read_trimmed_file(path: &PathBuf) -> Option<String> {
  if !path.exists() {
    return None;
  }
  fs::read_to_string(path)
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}
