use artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet;
use artcraft_client::utils::api_host::ApiHost;
use log::info;

/// Credentials for the upstream ArtCraft API.
///
/// Modern API/MCP clients authenticate with the secret in the HTTP
/// `Authorization` header. The backend accepts `Bearer <secret>`, `Key
/// <secret>`, or a bare secret; this client always emits the canonical
/// `Bearer` form. Browser/desktop sessions use the legacy cookie pair and are
/// kept as a compatibility fallback.
#[derive(Clone)]
pub enum ArtCraftCredentials {
    /// Either an MCP private session token (`mcp_session_...`) or an ArtCraft
    /// API key (`artcraft_api_...`). The backend distinguishes them by prefix.
    AuthorizationHeader(String),
    /// Legacy web/desktop cookie credentials.
    Cookies(StorytellerCredentialSet),
}

pub struct ArtCraftClient {
    pub api_host: ApiHost,
    pub credentials: Option<ArtCraftCredentials>,
}

impl ArtCraftClient {
    pub fn new(credentials: Option<ArtCraftCredentials>) -> Self {
        let api_host = ApiHost::Storyteller;
        info!("ArtCraft client initialized with host: Storyteller");
        Self {
            api_host,
            credentials,
        }
    }

    /// Apply the correct upstream authentication shape to a reqwest request.
    /// If an Authorization credential is present it strictly wins: upstream
    /// explicitly does not fall back to cookies when the header is present.
    pub fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.credentials {
            Some(ArtCraftCredentials::AuthorizationHeader(token)) => request.bearer_auth(token),
            Some(ArtCraftCredentials::Cookies(credentials)) => {
                if let Some(header) = credentials.maybe_as_cookie_header() {
                    request.header("Cookie", header)
                } else {
                    request
                }
            }
            None => request,
        }
    }

    pub fn absolute_url(&self, route_path: &str) -> String {
        format!(
            "{}{}",
            self.api_host.to_api_hostname_and_scheme(),
            route_path
        )
    }

}
