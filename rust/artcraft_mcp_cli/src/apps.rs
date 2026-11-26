//! Embedded MCP Apps (io.modelcontextprotocol/apps) — Phase 5.
//!
//! Four HTML views compiled into the binary with rust-embed and served as
//! `ui://` resources. Tools that produce visual output carry
//! `_meta.ui.resourceUri` so capable hosts render them inline; hosts without
//! apps support get plain text as before (graceful degradation).

use rmcp::model::*;
use rust_embed::Embed;
use serde_json::json;

#[derive(Embed)]
#[folder = "apps/"]
struct AppsAssets;

pub const EXT_KEY: &str = "io.modelcontextprotocol/apps";

pub struct AppDef {
    pub uri: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub description: &'static str,
}

pub const APPS: &[AppDef] = &[
    AppDef {
        uri: "ui://artcraft/media-gallery",
        name: "media-gallery",
        file: "media-gallery.html",
        description: "Grid gallery for browsing generated and uploaded media",
    },
    AppDef {
        uri: "ui://artcraft/media-player",
        name: "media-player",
        file: "media-player.html",
        description: "Inline video/audio player for generated media",
    },
    AppDef {
        uri: "ui://artcraft/viewer-3d",
        name: "viewer-3d",
        file: "viewer3d.html",
        description: "3D mesh and Gaussian splat preview",
    },
    AppDef {
        uri: "ui://artcraft/credit-dashboard",
        name: "credit-dashboard",
        file: "credit-dashboard.html",
        description: "Live credit balance dashboard",
    },
];

/// Tools and the app view their results render in.
pub fn app_for_tool(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "artcraft_list_media_files" | "artcraft_search_media" => "ui://artcraft/media-gallery",
        "artcraft_download_media_file" | "artcraft_generate_video" | "artcraft_generate_audio" => {
            "ui://artcraft/media-player"
        }
        "artcraft_generate_3d_object" | "artcraft_generate_splat" => "ui://artcraft/viewer-3d",
        "artcraft_get_credits" | "artcraft_estimate_cost" => "ui://artcraft/credit-dashboard",
        _ => return None,
    })
}

pub fn list_resources() -> ListResourcesResult {
    let resources = APPS
        .iter()
        .map(|app| {
            let mut r = Resource::new(app.uri, app.name);
            r.title = Some(app.description.to_string());
            r.description = Some(app.description.to_string());
            r.mime_type = Some("text/html".to_string());
            let mut meta = MetaObject::default();
            // Strict CSP: no network egress from the view except media embeds.
            meta.insert(
                "ui".to_string(),
                json!({
                    "csp": {
                        "defaultSrc": "'none'",
                        "mediaSrc": "https: data:",
                        "imgSrc": "https: data:",
                        "styleSrc": "'unsafe-inline'",
                        "scriptSrc": "'unsafe-inline'"
                    }
                }),
            );
            r.meta = Some(meta);
            r
        })
        .collect();
    ListResourcesResult {
        resources,
        ..Default::default()
    }
}

pub fn read_resource(uri: &str) -> Option<ReadResourceResponse> {
    let app = APPS.iter().find(|a| a.uri == uri)?;
    let asset = AppsAssets::get(app.file)?;
    let html = String::from_utf8_lossy(&asset.data).to_string();
    let contents = vec![ResourceContents::text(html, uri).with_mime_type("text/html;profile=mcp-app")];
    Some(ReadResourceResponse::Complete(ReadResourceResult::new(contents)))
}

/// Attach `_meta.ui.resourceUri` to a converted tool when an app exists.
pub fn annotate_tool_meta(tool: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let uri = app_for_tool(tool)?;
    let mut map = serde_json::Map::new();
    map.insert("ui".to_string(), json!({ "resourceUri": uri }));
    Some(map)
}
