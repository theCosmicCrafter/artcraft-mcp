//! Resource catalog beyond the embedded MCP apps.
//!
//! Real ArtCraft resources are local, deterministic documentation and catalog
//! views (no upstream API calls). The `test://` fixtures are the small static
//! resources required by the MCP conformance suite.

use rmcp::model::*;
use serde_json::json;

use super::fixtures;
use crate::{registry, tools};

const GETTING_STARTED: &str = "# Getting started with ArtCraft MCP\n\n\
1. Use `artcraft_get_session_info` or `artcraft_get_credits` to verify authentication.\n\
2. Use `artcraft_estimate_cost` before billable generation.\n\
3. Start with low-cost or free models, inspect the job result, then iterate.\n\
4. In progressive mode, use `artcraft_search_tools`, `artcraft_describe_tool`, and `artcraft_call_deferred_tool` for the full catalog.\n";

const COST_SAFETY: &str = "# ArtCraft cost safety\n\n\
Generation tools consume credits. Always preflight billable work with `artcraft_estimate_cost`.\n\n\
- Image generation is usually the cheapest iteration loop.\n\
- Video, 3D meshes, and Gaussian splats can be high-cost operations.\n\
- The MRTR confirmation gate seals a cost-approved retry in `requestState`; a declined confirmation never reaches the ArtCraft API.\n\
- Job tokens and media tokens are opaque identifiers, not credentials.\n";

fn resource(
    uri: &str,
    name: &str,
    description: &str,
    mime_type: &str,
    size: Option<u64>,
) -> Resource {
    let mut resource = Resource::new(uri, name)
        .with_description(description)
        .with_mime_type(mime_type);
    if let Some(size) = size {
        resource = resource.with_size(size);
    }
    resource
}

fn static_resources() -> Vec<Resource> {
    vec![
        resource(
            "artcraft://docs/getting-started",
            "artcraft-getting-started",
            "Local getting-started guide for the ArtCraft MCP tool catalog",
            "text/markdown",
            Some(GETTING_STARTED.len() as u64),
        ),
        resource(
            "artcraft://docs/cost-safety",
            "artcraft-cost-safety",
            "Credit safety and confirmation-gate guidance for generation workflows",
            "text/markdown",
            Some(COST_SAFETY.len() as u64),
        ),
        resource(
            "artcraft://examples/red-pixel",
            "artcraft-example-red-pixel",
            "Tiny embedded PNG example used to verify binary resource handling",
            "image/png",
            Some(69),
        ),
        resource(
            "artcraft://catalog/index",
            "artcraft-catalog-index",
            "Machine-readable index of the local ArtCraft tool and prompt catalog",
            "application/json",
            None,
        ),
    ]
}

pub fn list_resources() -> ListResourcesResult {
    let mut result = crate::apps::list_resources();
    result.resources.extend(static_resources());
    result
}

pub fn list_resource_templates() -> ListResourceTemplatesResult {
    ListResourceTemplatesResult::with_all_items(vec![
        ResourceTemplate::new(
            "artcraft://catalog/tools/{tool_name}",
            "artcraft-tool-catalog-entry",
        )
        .with_title("ArtCraft tool catalog entry")
        .with_description("Local schema and metadata for one ArtCraft MCP tool")
        .with_mime_type("application/json"),
        ResourceTemplate::new(
            "artcraft://catalog/prompts/{prompt_name}",
            "artcraft-prompt-catalog-entry",
        )
        .with_title("ArtCraft prompt catalog entry")
        .with_description("Local metadata for one ArtCraft prompt template")
        .with_mime_type("application/json"),
        // Canonical fixture template required by the conformance suite. It is
        // harmless, deterministic, and useful when testing RFC 6570 clients.
        ResourceTemplate::new("test://template/{id}/data", "test-template-data")
            .with_title("MCP template fixture")
            .with_description("Deterministic template fixture used by MCP conformance tests")
            .with_mime_type("application/json"),
    ])
}

fn finish_read(contents: Vec<ResourceContents>) -> ReadResourceResponse {
    ReadResourceResponse::Complete(
        ReadResourceResult::new(contents)
            .with_ttl_ms(300_000)
            .with_cache_scope(CacheScope::Public),
    )
}

fn text_contents(uri: &str, text: impl Into<String>, mime_type: &str) -> Vec<ResourceContents> {
    vec![ResourceContents::text(text, uri).with_mime_type(mime_type)]
}

fn catalog_tool_json(tool_name: &str) -> Option<String> {
    let tool = tools::get_all_tools()
        .into_iter()
        .find(|tool| tool.name == tool_name)?;
    let meta = registry::meta_for(tool_name);
    let value = json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": tool.input_schema,
        "category": meta.map(|m| m.category.as_str()),
        "costTier": meta.map(|m| m.cost.as_str()),
        "asyncJob": meta.map(|m| m.async_job),
        "tags": meta.map(|m| m.tags),
    });
    Some(value.to_string())
}

fn catalog_index_json() -> String {
    let tools = tools::get_all_tools()
        .into_iter()
        .map(|tool| {
            let meta = registry::meta_for(&tool.name);
            json!({
                "name": tool.name,
                "description": tool.description,
                "category": meta.map(|m| m.category.as_str()),
                "costTier": meta.map(|m| m.cost.as_str()),
                "asyncJob": meta.map(|m| m.async_job),
                "resourceUri": format!("artcraft://catalog/tools/{}", tool.name),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "tools": tools,
        "prompts": super::prompts::prompt_catalog_json()["prompts"].clone(),
    })
    .to_string()
}

fn prompt_catalog_entry_json(prompt_name: &str) -> Option<String> {
    let prompts = super::prompts::prompt_catalog_json()["prompts"].clone();
    let entry = prompts
        .as_array()?
        .iter()
        .find(|prompt| prompt.get("name").and_then(|name| name.as_str()) == Some(prompt_name))?;
    Some(entry.to_string())
}

fn read_artcraft_resource(uri: &str) -> Option<ReadResourceResponse> {
    match uri {
        "artcraft://docs/getting-started" => Some(finish_read(text_contents(
            uri,
            GETTING_STARTED,
            "text/markdown",
        ))),
        "artcraft://docs/cost-safety" => Some(finish_read(text_contents(
            uri,
            COST_SAFETY,
            "text/markdown",
        ))),
        "artcraft://examples/red-pixel" => Some(finish_read(vec![ResourceContents::blob(
            fixtures::PNG_1X1_RED_BASE64,
            uri,
        )
        .with_mime_type("image/png")])),
        "artcraft://catalog/index" => Some(finish_read(text_contents(
            uri,
            catalog_index_json(),
            "application/json",
        ))),
        _ => {
            if let Some(tool_name) = uri.strip_prefix("artcraft://catalog/tools/") {
                if !tool_name.is_empty() && !tool_name.contains(['/', '?', '#']) {
                    return catalog_tool_json(tool_name)
                        .map(|text| finish_read(text_contents(uri, text, "application/json")));
                }
            }
            if let Some(prompt_name) = uri.strip_prefix("artcraft://catalog/prompts/") {
                if !prompt_name.is_empty() && !prompt_name.contains(['/', '?', '#']) {
                    return prompt_catalog_entry_json(prompt_name)
                        .map(|text| finish_read(text_contents(uri, text, "application/json")));
                }
            }
            None
        }
    }
}

fn read_protocol_fixture_resource(uri: &str) -> Option<ReadResourceResponse> {
    match uri {
        "test://static-text" => Some(finish_read(text_contents(
            uri,
            "This is the content of the static text resource.",
            "text/plain",
        ))),
        "test://static-binary" => Some(finish_read(vec![ResourceContents::blob(
            fixtures::PNG_1X1_RED_BASE64,
            uri,
        )
        .with_mime_type("image/png")])),
        _ => {
            let id = uri
                .strip_prefix("test://template/")
                .and_then(|rest| rest.strip_suffix("/data"))
                .filter(|id| !id.is_empty() && !id.contains('/'))?;
            let text = json!({
                "id": id,
                "templateTest": true,
                "data": format!("Data for ID: {id}"),
            })
            .to_string();
            Some(finish_read(text_contents(uri, text, "application/json")))
        }
    }
}

pub fn read_resource(uri: &str) -> Option<ReadResourceResponse> {
    if let Some(response) = crate::apps::read_resource(uri) {
        // Embedded apps are static public assets as well; add the modern
        // caching metadata uniformly rather than modifying the apps module.
        if let ReadResourceResponse::Complete(result) = response {
            return Some(ReadResourceResponse::Complete(
                result
                    .with_ttl_ms(300_000)
                    .with_cache_scope(CacheScope::Public),
            ));
        }
        return Some(response);
    }
    read_artcraft_resource(uri).or_else(|| read_protocol_fixture_resource(uri))
}

/// Whether a URI is one this server can plausibly publish update
/// notifications for. Static resources never change at runtime, but accepting
/// subscriptions for them is spec-valid because their update event set is
/// empty. Unknown URIs are filtered out for modern transport-neutral
/// subscriptions.
pub fn subscription_uri_supported(uri: &str) -> bool {
    crate::apps::APPS.iter().any(|app| app.uri == uri)
        || static_resources()
            .iter()
            .any(|resource| resource.uri == uri)
        || uri == "test://static-text"
        || uri == "test://static-binary"
        || uri
            .strip_prefix("test://template/")
            .and_then(|rest| rest.strip_suffix("/data"))
            .is_some_and(|id| !id.is_empty() && !id.contains('/'))
        || uri
            .strip_prefix("artcraft://catalog/tools/")
            .is_some_and(|name| {
                !name.is_empty() && tools::get_all_tools().iter().any(|tool| tool.name == name)
            })
        || uri
            .strip_prefix("artcraft://catalog/prompts/")
            .is_some_and(|name| !name.is_empty() && prompt_catalog_entry_json(name).is_some())
}

pub fn resource_completion_candidates(uri_template: &str, argument_name: &str) -> Vec<String> {
    match (uri_template, argument_name) {
        ("artcraft://catalog/tools/{tool_name}", "tool_name") => tools::get_all_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect(),
        ("artcraft://catalog/prompts/{prompt_name}", "prompt_name") => {
            super::prompts::prompt_catalog_json()["prompts"]
                .as_array()
                .map(|prompts| {
                    prompts
                        .iter()
                        .filter_map(|prompt| {
                            prompt
                                .get("name")
                                .and_then(|name| name.as_str())
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        ("test://template/{id}/data", "id") => {
            vec!["123".to_string(), "456".to_string(), "789".to_string()]
        }
        _ => Vec::new(),
    }
}
