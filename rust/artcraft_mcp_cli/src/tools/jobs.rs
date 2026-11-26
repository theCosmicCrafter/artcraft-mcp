use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_delete, raw_json_get};
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_get_job_status".to_string(),
            description: "Poll the status of a single inference job by token.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": { "job_token": { "type": "string" } },
                "required": ["job_token"]
            })),
        },
        Tool {
            name: "artcraft_list_jobs".to_string(),
            description:
                "List recent jobs for the current authenticated or anonymous visitor session."
                    .to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["pending", "started", "completed", "failed", "cancelled"] },
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer" }
                }
            })),
        },
        Tool {
            name: "artcraft_terminate_job".to_string(),
            description: "Cancel a running or pending job owned by this user/session.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": { "job_token": { "type": "string" } },
                "required": ["job_token"]
            })),
        },
    ]
}

fn nested_string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

pub async fn get_job_status(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let job_token = arguments["job_token"]
        .as_str()
        .ok_or_else(|| anyhow!("job_token is required"))?;

    let url = client.absolute_url(&format!("/v1/jobs/job/{}", job_token));
    let response = raw_json_get(&url, client).await?;
    let job = response.get("state").unwrap_or(&response);
    let status = nested_string(job, &["status", "status"])
        .or_else(|| response.get("status").and_then(|v| v.as_str()))
        .unwrap_or("?");
    let progress = job
        .pointer("/status/progress_percentage")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let category = nested_string(job, &["request", "inference_category"]).unwrap_or("?");

    let mut lines = vec![
        format!("Job: {}", job_token),
        format!("Status: {}", status),
        format!("Progress: {}%", progress),
        format!("Category: {}", category),
    ];
    if let Some(extra) = nested_string(job, &["status", "maybe_extra_status_description"]) {
        lines.push(format!("Extra: {}", extra));
    }
    if let Some(failure) = nested_string(job, &["status", "maybe_failure_message"]) {
        lines.push(format!("Failure: {}", failure));
    }
    if let Some(result) = nested_string(job, &["maybe_result", "entity_token"]) {
        lines.push(format!("Result token: {}", result));
    }

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: lines.join("\n"),
    }])
}

pub async fn list_jobs(_arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let url = client.absolute_url("/v1/jobs/session");
    let response = raw_json_get(&url, client).await?;
    let empty = Vec::new();
    let jobs = response
        .get("jobs")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let mut lines = vec![format!("Found {} jobs:", jobs.len())];
    for job in jobs {
        let token = job.get("job_token").and_then(|v| v.as_str()).unwrap_or("?");
        let status = nested_string(job, &["status", "status"]).unwrap_or("?");
        let progress = job
            .pointer("/status/progress_percentage")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let category = nested_string(job, &["request", "inference_category"]).unwrap_or("?");
        let created = job
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        lines.push(format!(
            "  - {} | {} | {}% | {} | {}",
            token, status, progress, category, created
        ));
    }

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: lines.join("\n"),
    }])
}

pub async fn terminate_job(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let job_token = arguments["job_token"]
        .as_str()
        .ok_or_else(|| anyhow!("job_token is required"))?;
    let url = client.absolute_url(&format!("/v1/jobs/job/{}", job_token));
    raw_json_delete(&url, json!({}), client).await?;
    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Job {} termination requested.", job_token),
    }])
}
