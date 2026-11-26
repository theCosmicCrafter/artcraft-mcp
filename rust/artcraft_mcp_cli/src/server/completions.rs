//! Minimal but useful `completion/complete` implementation.
//!
//! MCP completion references are prompt templates and resource templates. For
//! ArtCraft, the useful completion surface is enum-like prompt arguments plus
//! tool/prompt names behind the local catalog resource templates.

use rmcp::model::*;

use super::{prompts, resources_ext};

fn filter_prefix(candidates: Vec<String>, prefix: &str) -> Vec<String> {
    let prefix = prefix.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| candidate.to_ascii_lowercase().starts_with(&prefix))
        .filter(|candidate| seen.insert(candidate.clone()))
        .take(CompletionInfo::MAX_VALUES)
        .collect()
}

pub fn complete(request: CompleteRequestParams) -> CompleteResult {
    let argument_name = request.argument.name.as_str();
    let prefix = request.argument.value.as_str();
    let candidates: Vec<String> = match &request.r#ref {
        Reference::Prompt(prompt_ref) => {
            prompts::prompt_completion_candidates(&prompt_ref.name, argument_name)
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        Reference::Resource(resource_ref) => {
            resources_ext::resource_completion_candidates(&resource_ref.uri, argument_name)
        }
        _ => Vec::new(),
    };
    let values = filter_prefix(candidates, prefix);
    let completion = CompletionInfo::with_all_values(values).unwrap_or_default();
    CompleteResult::new(completion)
}
