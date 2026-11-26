use anyhow::{anyhow, Result};
use log::error;
use serde_json::Value;

use crate::client::ArtCraftClient;
use crate::types::{Tool, ToolContent};

mod account;
mod animation;
mod api_keys;
mod characters;
mod conversion;
mod cost;
mod craft_apps;
mod edit;
mod featured;
mod folders;
mod generate;
mod http;
mod jobs;
mod mcp_sessions;
mod media;
mod prompts;
mod referrals;
mod social;
mod studio;
mod tts;
mod weights;

pub fn get_all_tools() -> Vec<Tool> {
    let mut tools = Vec::new();

    tools.extend(generate::tools());
    tools.extend(edit::tools());
    tools.extend(cost::tools());
    tools.extend(media::tools());
    tools.extend(jobs::tools());
    tools.extend(characters::tools());
    tools.extend(prompts::tools());
    tools.extend(tts::tools());
    tools.extend(weights::tools());
    tools.extend(account::tools());
    tools.extend(social::tools());
    tools.extend(referrals::tools());
    tools.extend(folders::tools());
    tools.extend(api_keys::tools());
    tools.extend(mcp_sessions::tools());
    tools.extend(studio::tools());
    tools.extend(animation::tools());
    tools.extend(conversion::tools());
    tools.extend(craft_apps::tools());
    tools.extend(featured::tools());

    tools
}

pub async fn execute_tool(
    name: &str,
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    match name {
        // Generate
        "artcraft_generate_image" => generate::generate_image(arguments, client).await,
        "artcraft_generate_video" => generate::generate_video(arguments, client).await,
        "artcraft_generate_audio" => generate::generate_audio(arguments, client).await,
        "artcraft_generate_3d_object" => generate::generate_3d_object(arguments, client).await,
        "artcraft_generate_splat" => generate::generate_splat(arguments, client).await,

        // Edit
        "artcraft_edit_image" => edit::edit_image(arguments, client).await,
        "artcraft_remove_background" => edit::remove_background(arguments, client).await,
        "artcraft_inpaint_image" => edit::inpaint_image(arguments, client).await,
        "artcraft_edit_image_angle" => edit::edit_image_angle(arguments, client).await,
        "artcraft_edit_video" => edit::edit_video(arguments, client).await,

        // Cost & Models
        "artcraft_estimate_cost" => cost::estimate_cost(arguments, client).await,
        "artcraft_estimate_splat_cost" => cost::estimate_splat_cost(arguments, client).await,
        "artcraft_list_image_models" => cost::list_image_models(arguments, client).await,
        "artcraft_list_video_models" => cost::list_video_models(arguments, client).await,

        // Media
        "artcraft_upload_image" => media::upload_image(arguments, client).await,
        "artcraft_upload_video" => media::upload_video(arguments, client).await,
        "artcraft_upload_audio" => media::upload_audio(arguments, client).await,
        "artcraft_get_media_file" => media::get_media_file(arguments, client).await,
        "artcraft_download_media_file" => media::download_media_file(arguments, client).await,
        "artcraft_list_media_files" => media::list_media_files(arguments, client).await,
        "artcraft_search_media" => media::search_media(arguments, client).await,
        "artcraft_delete_media_file" => media::delete_media_file(arguments, client).await,
        "artcraft_rename_media_file" => media::rename_media_file(arguments, client).await,
        "artcraft_set_media_visibility" => media::set_media_visibility(arguments, client).await,

        // Jobs
        "artcraft_get_job_status" => jobs::get_job_status(arguments, client).await,
        "artcraft_list_jobs" => jobs::list_jobs(arguments, client).await,
        "artcraft_terminate_job" => jobs::terminate_job(arguments, client).await,

        // Characters
        "artcraft_create_character" => characters::create_character(arguments, client).await,
        "artcraft_list_characters" => characters::list_characters(arguments, client).await,
        "artcraft_get_character" => characters::get_character(arguments, client).await,
        "artcraft_delete_character" => characters::delete_character(arguments, client).await,

        // Prompts
        "artcraft_create_prompt" => prompts::create_prompt(arguments, client).await,
        "artcraft_get_prompt" => prompts::get_prompt(arguments, client).await,

        // TTS
        "artcraft_tts_generate" => tts::tts_generate(arguments, client).await,
        "artcraft_tts_search_models" => tts::tts_search_models(arguments, client).await,

        // Weights
        "artcraft_list_weights" => weights::list_weights(arguments, client).await,
        "artcraft_search_weights" => weights::search_weights(arguments, client).await,
        "artcraft_get_weight" => weights::get_weight(arguments, client).await,
        "artcraft_delete_weight" => weights::delete_weight(arguments, client).await,

        // Account & Billing
        "artcraft_get_session_info" => account::get_session_info(arguments, client).await,
        "artcraft_get_credits" => account::get_credits(arguments, client).await,
        "artcraft_get_subscription" => account::get_subscription(arguments, client).await,
        "artcraft_create_checkout_session" => {
            account::create_checkout_session(arguments, client).await
        }
        "artcraft_create_subscription_checkout" => {
            account::create_subscription_checkout(arguments, client).await
        }
        "artcraft_get_billing_portal_url" => {
            account::get_billing_portal_url(arguments, client).await
        }
        "artcraft_get_billing_cancel_plan_url" => {
            account::get_billing_cancel_plan_url(arguments, client).await
        }
        "artcraft_get_billing_switch_plan_url" => {
            account::get_billing_switch_plan_url(arguments, client).await
        }
        "artcraft_get_billing_update_payment_method_url" => {
            account::get_billing_update_payment_method_url(arguments, client).await
        }
        "artcraft_set_unsubscribe_reason" => {
            account::set_unsubscribe_reason(arguments, client).await
        }

        // Social
        "artcraft_create_bookmark" => social::create_bookmark(arguments, client).await,
        "artcraft_list_bookmarks" => social::list_bookmarks(arguments, client).await,
        "artcraft_delete_bookmark" => social::delete_bookmark(arguments, client).await,
        "artcraft_rate_media" => social::rate_media(arguments, client).await,
        "artcraft_create_comment" => social::create_comment(arguments, client).await,
        "artcraft_list_comments" => social::list_comments(arguments, client).await,
        "artcraft_delete_comment" => social::delete_comment(arguments, client).await,
        "artcraft_list_tags" => social::list_tags(arguments, client).await,
        "artcraft_set_tags" => social::set_tags(arguments, client).await,

        // Referrals
        "artcraft_create_referral_code" => referrals::create_referral_code(arguments, client).await,
        "artcraft_list_referral_codes" => referrals::list_referral_codes(arguments, client).await,

        // Folders
        "artcraft_list_folders" => folders::list_folders(arguments, client).await,
        "artcraft_create_folder" => folders::create_folder(arguments, client).await,
        "artcraft_get_folder" => folders::get_folder(arguments, client).await,
        "artcraft_delete_folder" => folders::delete_folder(arguments, client).await,
        "artcraft_rename_folder" => folders::rename_folder(arguments, client).await,
        "artcraft_set_folder_star" => folders::set_folder_star(arguments, client).await,
        "artcraft_set_folder_color" => folders::set_folder_color(arguments, client).await,
        "artcraft_set_folder_cover_image" => {
            folders::set_folder_cover_image(arguments, client).await
        }
        "artcraft_list_subfolders" => folders::list_subfolders(arguments, client).await,
        "artcraft_add_subfolders" => folders::add_subfolders(arguments, client).await,
        "artcraft_remove_subfolders" => folders::remove_subfolders(arguments, client).await,
        "artcraft_list_folder_media_files" => {
            folders::list_folder_media_files(arguments, client).await
        }
        "artcraft_list_media_files_without_folder" => {
            folders::list_media_files_without_folder(arguments, client).await
        }
        "artcraft_add_media_to_folder" => folders::add_media_to_folder(arguments, client).await,
        "artcraft_remove_media_from_folder" => {
            folders::remove_media_from_folder(arguments, client).await
        }
        "artcraft_move_media_to_folder" => folders::move_media_to_folder(arguments, client).await,

        // API keys
        "artcraft_create_api_key" => api_keys::create_api_key(arguments, client).await,
        "artcraft_list_api_keys" => api_keys::list_api_keys(arguments, client).await,
        "artcraft_get_api_key" => api_keys::get_api_key(arguments, client).await,
        "artcraft_update_api_key" => api_keys::update_api_key(arguments, client).await,
        "artcraft_delete_api_key" => api_keys::delete_api_key(arguments, client).await,

        // MCP sessions
        "artcraft_create_mcp_session" => mcp_sessions::create_mcp_session(arguments, client).await,
        "artcraft_list_mcp_sessions" => mcp_sessions::list_mcp_sessions(arguments, client).await,
        "artcraft_refresh_mcp_session" => {
            mcp_sessions::refresh_mcp_session(arguments, client).await
        }
        "artcraft_revoke_mcp_session" => mcp_sessions::revoke_mcp_session(arguments, client).await,
        "artcraft_delete_mcp_session" => mcp_sessions::delete_mcp_session(arguments, client).await,

        // Studio
        "artcraft_upload_studio_scene_snapshot" => {
            studio::upload_studio_scene_snapshot(arguments, client).await
        }
        "artcraft_upload_studio_shot" => studio::upload_studio_shot(arguments, client).await,

        // Animation & conversion
        "artcraft_create_face_animation" => {
            animation::create_face_animation(arguments, client).await
        }
        "artcraft_create_rerender_animation" => {
            animation::create_rerender_animation(arguments, client).await
        }
        "artcraft_convert_fbx_to_gltf" => conversion::convert_fbx_to_gltf(arguments, client).await,
        "artcraft_convert_render_engine_scene" => {
            conversion::convert_render_engine_scene(arguments, client).await
        }
        "artcraft_convert_bvh_to_workflow" => {
            conversion::convert_bvh_to_workflow(arguments, client).await
        }

        // Craft apps, featured items, and beta keys
        "artcraft_get_craft_apps_release_info" => {
            craft_apps::get_craft_apps_release_info(arguments, client).await
        }
        "artcraft_list_featured_media" => featured::list_featured_media(arguments, client).await,
        "artcraft_search_featured_media" => {
            featured::search_featured_media(arguments, client).await
        }
        "artcraft_is_featured_item" => featured::is_featured_item(arguments, client).await,
        "artcraft_list_beta_keys" => featured::list_beta_keys(arguments, client).await,
        "artcraft_redeem_beta_key" => featured::redeem_beta_key(arguments, client).await,

        _ => {
            error!("Unknown tool: {}", name);
            Err(anyhow!("Unknown tool: {}", name))
        }
    }
}
