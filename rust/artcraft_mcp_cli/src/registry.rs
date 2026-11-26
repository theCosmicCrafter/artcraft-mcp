//! Tool registry metadata: category, cost tier, annotations, and discovery
//! indexes for the business-tool catalog. Drives progressive discovery and the
//! canonical ordering/caching hints on tools/list.

use rmcp::model::ToolAnnotations;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Account,
    Audio,
    Character,
    Community,
    Files,
    Image,
    Jobs,
    Lora,
    Mesh3d,
    Splat,
    Video,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Account => "account",
            Category::Audio => "audio",
            Category::Character => "character",
            Category::Community => "community",
            Category::Files => "files",
            Category::Image => "image",
            Category::Jobs => "jobs",
            Category::Lora => "lora",
            Category::Mesh3d => "3d",
            Category::Splat => "splat",
            Category::Video => "video",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostTier {
    Free,
    Low,
    Medium,
    High,
}

impl CostTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            CostTier::Free => "free",
            CostTier::Low => "low",
            CostTier::Medium => "medium",
            CostTier::High => "high",
        }
    }
}

pub struct ToolMeta {
    pub name: &'static str,
    pub category: Category,
    pub cost: CostTier,
    /// Starts a long-running async job on the ArtCraft backend.
    pub async_job: bool,
    pub tags: &'static [&'static str],
}

impl ToolMeta {
    pub fn annotations(&self) -> ToolAnnotations {
        // Read-only means the tool does not mutate any server-side state. Cost
        // alone is not the signal — deletion and mutation tools are free but destructive.
        let name = self.name;
        let read_only = name.starts_with("artcraft_list")
            || name.starts_with("artcraft_get")
            || name.starts_with("artcraft_search")
            || name.starts_with("artcraft_estimate")
            || name.contains("_search")
            || name == "artcraft_is_featured_item"
            || name == "artcraft_describe_tool"
            || name == "artcraft_search_tools";
        // Destructive covers deletion/termination plus operations that
        // overwrite or remove existing data. Media downloads overwrite host files.
        let destructive = name.contains("delete")
            || name.contains("terminate")
            || name.contains("download")
            || name.contains("update")
            || name.contains("revoke")
            || name.starts_with("artcraft_remove_media_from_folder")
            || name.starts_with("artcraft_remove_subfolders");
        // Mutations are not idempotent except where upstream explicitly makes
        // them so; be conservative. Read-only calls are idempotent.
        let idempotent = read_only;
        ToolAnnotations::from_raw(
            None,
            Some(read_only),
            Some(destructive),
            Some(idempotent),
            Some(true),
        )
    }
}

/// Metadata for every business tool. Hot tools for progressive mode are the generators,
/// cost/credit checkers, and job pollers listed in the Phase 2 blueprint.
pub const REGISTRY: &[ToolMeta] = &[
    // Image generation & editing
    ToolMeta {
        name: "artcraft_generate_image",
        category: Category::Image,
        cost: CostTier::Medium,
        async_job: true,
        tags: &[
            "generate",
            "image",
            "flux",
            "gpt-image",
            "seedream",
            "midjourney",
        ],
    },
    ToolMeta {
        name: "artcraft_edit_image",
        category: Category::Image,
        cost: CostTier::Medium,
        async_job: true,
        tags: &["edit", "image"],
    },
    ToolMeta {
        name: "artcraft_edit_image_angle",
        category: Category::Image,
        cost: CostTier::Medium,
        async_job: true,
        tags: &["edit", "image", "angle", "camera"],
    },
    ToolMeta {
        name: "artcraft_inpaint_image",
        category: Category::Image,
        cost: CostTier::Medium,
        async_job: true,
        tags: &["inpaint", "mask", "edit"],
    },
    ToolMeta {
        name: "artcraft_remove_background",
        category: Category::Image,
        cost: CostTier::Low,
        async_job: true,
        tags: &["background", "cutout"],
    },
    // Video
    ToolMeta {
        name: "artcraft_generate_video",
        category: Category::Video,
        cost: CostTier::High,
        async_job: true,
        tags: &[
            "generate", "video", "seedance", "kling", "veo", "sora", "wan",
        ],
    },
    ToolMeta {
        name: "artcraft_edit_video",
        category: Category::Video,
        cost: CostTier::High,
        async_job: true,
        tags: &["edit", "video", "switchx"],
    },
    // Audio & TTS
    ToolMeta {
        name: "artcraft_generate_audio",
        category: Category::Audio,
        cost: CostTier::Medium,
        async_job: true,
        tags: &["generate", "audio", "music"],
    },
    ToolMeta {
        name: "artcraft_tts_generate",
        category: Category::Audio,
        cost: CostTier::Low,
        async_job: true,
        tags: &["tts", "speech", "text-to-speech"],
    },
    ToolMeta {
        name: "artcraft_tts_search_models",
        category: Category::Audio,
        cost: CostTier::Free,
        async_job: false,
        tags: &["tts", "models", "search"],
    },
    ToolMeta {
        name: "artcraft_upload_audio",
        category: Category::Audio,
        cost: CostTier::Free,
        async_job: false,
        tags: &["upload", "audio"],
    },
    // 3D & splat
    ToolMeta {
        name: "artcraft_generate_3d_object",
        category: Category::Mesh3d,
        cost: CostTier::High,
        async_job: true,
        tags: &["3d", "mesh", "hunyuan", "tripo", "meshy", "rodin", "glb"],
    },
    ToolMeta {
        name: "artcraft_generate_splat",
        category: Category::Splat,
        cost: CostTier::High,
        async_job: true,
        tags: &["splat", "gaussian", "marble", "triposplat"],
    },
    ToolMeta {
        name: "artcraft_estimate_splat_cost",
        category: Category::Splat,
        cost: CostTier::Free,
        async_job: false,
        tags: &["cost", "estimate", "splat"],
    },
    // Cost & account
    ToolMeta {
        name: "artcraft_estimate_cost",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["cost", "estimate", "credits", "preflight"],
    },
    ToolMeta {
        name: "artcraft_get_credits",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["credits", "balance"],
    },
    ToolMeta {
        name: "artcraft_get_session_info",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["account", "session"],
    },
    ToolMeta {
        name: "artcraft_get_subscription",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["subscription", "plan"],
    },
    ToolMeta {
        name: "artcraft_get_billing_portal_url",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["billing", "stripe"],
    },
    ToolMeta {
        name: "artcraft_get_billing_cancel_plan_url",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["billing", "stripe", "cancel"],
    },
    ToolMeta {
        name: "artcraft_get_billing_switch_plan_url",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["billing", "stripe", "switch", "plan"],
    },
    ToolMeta {
        name: "artcraft_get_billing_update_payment_method_url",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["billing", "stripe", "payment"],
    },
    ToolMeta {
        name: "artcraft_set_unsubscribe_reason",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["billing", "feedback", "unsubscribe"],
    },
    ToolMeta {
        name: "artcraft_create_checkout_session",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["checkout", "credits", "purchase"],
    },
    ToolMeta {
        name: "artcraft_create_subscription_checkout",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["checkout", "subscription"],
    },
    // Jobs
    ToolMeta {
        name: "artcraft_get_job_status",
        category: Category::Jobs,
        cost: CostTier::Free,
        async_job: false,
        tags: &["job", "status", "poll"],
    },
    ToolMeta {
        name: "artcraft_list_jobs",
        category: Category::Jobs,
        cost: CostTier::Free,
        async_job: false,
        tags: &["job", "list"],
    },
    ToolMeta {
        name: "artcraft_terminate_job",
        category: Category::Jobs,
        cost: CostTier::Free,
        async_job: false,
        tags: &["job", "cancel", "terminate"],
    },
    // Media files
    ToolMeta {
        name: "artcraft_upload_image",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["upload", "image"],
    },
    ToolMeta {
        name: "artcraft_upload_video",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["upload", "video"],
    },
    ToolMeta {
        name: "artcraft_download_media_file",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["download", "media"],
    },
    ToolMeta {
        name: "artcraft_get_media_file",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "info"],
    },
    ToolMeta {
        name: "artcraft_list_media_files",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "list", "gallery"],
    },
    ToolMeta {
        name: "artcraft_search_media",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "search"],
    },
    ToolMeta {
        name: "artcraft_delete_media_file",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "delete"],
    },
    ToolMeta {
        name: "artcraft_rename_media_file",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "rename"],
    },
    ToolMeta {
        name: "artcraft_set_media_visibility",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "visibility"],
    },
    ToolMeta {
        name: "artcraft_rate_media",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "rate"],
    },
    ToolMeta {
        name: "artcraft_set_tags",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "tags"],
    },
    ToolMeta {
        name: "artcraft_list_tags",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["media", "tags", "list"],
    },
    // Community, prompts, characters, weights
    ToolMeta {
        name: "artcraft_create_prompt",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["prompt", "create"],
    },
    ToolMeta {
        name: "artcraft_get_prompt",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["prompt", "get"],
    },
    ToolMeta {
        name: "artcraft_create_character",
        category: Category::Character,
        cost: CostTier::Free,
        async_job: false,
        tags: &["character", "create"],
    },
    ToolMeta {
        name: "artcraft_list_characters",
        category: Category::Character,
        cost: CostTier::Free,
        async_job: false,
        tags: &["character", "list"],
    },
    ToolMeta {
        name: "artcraft_get_character",
        category: Category::Character,
        cost: CostTier::Free,
        async_job: false,
        tags: &["character", "get"],
    },
    ToolMeta {
        name: "artcraft_delete_character",
        category: Category::Character,
        cost: CostTier::Free,
        async_job: false,
        tags: &["character", "delete"],
    },
    ToolMeta {
        name: "artcraft_list_weights",
        category: Category::Lora,
        cost: CostTier::Free,
        async_job: false,
        tags: &["lora", "weights", "list"],
    },
    ToolMeta {
        name: "artcraft_get_weight",
        category: Category::Lora,
        cost: CostTier::Free,
        async_job: false,
        tags: &["lora", "weights", "get"],
    },
    ToolMeta {
        name: "artcraft_search_weights",
        category: Category::Lora,
        cost: CostTier::Free,
        async_job: false,
        tags: &["lora", "weights", "search"],
    },
    ToolMeta {
        name: "artcraft_delete_weight",
        category: Category::Lora,
        cost: CostTier::Free,
        async_job: false,
        tags: &["lora", "weights", "delete"],
    },
    ToolMeta {
        name: "artcraft_list_bookmarks",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["bookmarks"],
    },
    ToolMeta {
        name: "artcraft_create_bookmark",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["bookmarks", "create"],
    },
    ToolMeta {
        name: "artcraft_delete_bookmark",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["bookmarks", "delete"],
    },
    ToolMeta {
        name: "artcraft_list_comments",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["comments"],
    },
    ToolMeta {
        name: "artcraft_create_comment",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["comments", "create"],
    },
    ToolMeta {
        name: "artcraft_delete_comment",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["comments", "delete"],
    },
    ToolMeta {
        name: "artcraft_list_referral_codes",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["referral"],
    },
    ToolMeta {
        name: "artcraft_create_referral_code",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["referral", "create"],
    },
    ToolMeta {
        name: "artcraft_list_image_models",
        category: Category::Image,
        cost: CostTier::Free,
        async_job: false,
        tags: &["models", "image", "list"],
    },
    ToolMeta {
        name: "artcraft_list_video_models",
        category: Category::Video,
        cost: CostTier::Free,
        async_job: false,
        tags: &["models", "video", "list"],
    },
    // Folders
    ToolMeta {
        name: "artcraft_list_folders",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "list", "organization"],
    },
    ToolMeta {
        name: "artcraft_create_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "create", "organization"],
    },
    ToolMeta {
        name: "artcraft_get_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "get"],
    },
    ToolMeta {
        name: "artcraft_delete_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "delete"],
    },
    ToolMeta {
        name: "artcraft_rename_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "rename"],
    },
    ToolMeta {
        name: "artcraft_set_folder_star",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "star", "favorite"],
    },
    ToolMeta {
        name: "artcraft_set_folder_color",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "color"],
    },
    ToolMeta {
        name: "artcraft_set_folder_cover_image",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "cover", "image"],
    },
    ToolMeta {
        name: "artcraft_list_subfolders",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "subfolders", "list"],
    },
    ToolMeta {
        name: "artcraft_add_subfolders",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "subfolders", "add"],
    },
    ToolMeta {
        name: "artcraft_remove_subfolders",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "subfolders", "remove"],
    },
    ToolMeta {
        name: "artcraft_list_folder_media_files",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "media", "list"],
    },
    ToolMeta {
        name: "artcraft_list_media_files_without_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "media", "unfoldered"],
    },
    ToolMeta {
        name: "artcraft_add_media_to_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "media", "add"],
    },
    ToolMeta {
        name: "artcraft_remove_media_from_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "media", "remove"],
    },
    ToolMeta {
        name: "artcraft_move_media_to_folder",
        category: Category::Files,
        cost: CostTier::Free,
        async_job: false,
        tags: &["folders", "media", "move"],
    },
    // API keys and MCP sessions
    ToolMeta {
        name: "artcraft_create_api_key",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["api", "key", "credentials", "create"],
    },
    ToolMeta {
        name: "artcraft_list_api_keys",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["api", "key", "credentials", "list"],
    },
    ToolMeta {
        name: "artcraft_get_api_key",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["api", "key", "credentials", "get"],
    },
    ToolMeta {
        name: "artcraft_update_api_key",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["api", "key", "credentials", "update"],
    },
    ToolMeta {
        name: "artcraft_delete_api_key",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["api", "key", "credentials", "delete"],
    },
    ToolMeta {
        name: "artcraft_create_mcp_session",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["mcp", "session", "credentials", "create"],
    },
    ToolMeta {
        name: "artcraft_list_mcp_sessions",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["mcp", "session", "credentials", "list"],
    },
    ToolMeta {
        name: "artcraft_refresh_mcp_session",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["mcp", "session", "credentials", "refresh"],
    },
    ToolMeta {
        name: "artcraft_revoke_mcp_session",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["mcp", "session", "credentials", "revoke"],
    },
    ToolMeta {
        name: "artcraft_delete_mcp_session",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["mcp", "session", "credentials", "delete"],
    },
    // Studio, animation, conversion
    ToolMeta {
        name: "artcraft_upload_studio_scene_snapshot",
        category: Category::Image,
        cost: CostTier::Free,
        async_job: false,
        tags: &["studio", "snapshot", "scene", "upload"],
    },
    ToolMeta {
        name: "artcraft_upload_studio_shot",
        category: Category::Video,
        cost: CostTier::Free,
        async_job: false,
        tags: &["studio", "shot", "frames", "upload"],
    },
    ToolMeta {
        name: "artcraft_create_face_animation",
        category: Category::Video,
        cost: CostTier::High,
        async_job: true,
        tags: &["animation", "face", "lipsync", "sadtalker"],
    },
    ToolMeta {
        name: "artcraft_create_rerender_animation",
        category: Category::Video,
        cost: CostTier::High,
        async_job: true,
        tags: &["animation", "rerender", "video", "legacy"],
    },
    ToolMeta {
        name: "artcraft_convert_fbx_to_gltf",
        category: Category::Mesh3d,
        cost: CostTier::High,
        async_job: true,
        tags: &["conversion", "fbx", "gltf", "3d"],
    },
    ToolMeta {
        name: "artcraft_convert_render_engine_scene",
        category: Category::Mesh3d,
        cost: CostTier::High,
        async_job: true,
        tags: &["conversion", "scene", "render", "video"],
    },
    ToolMeta {
        name: "artcraft_convert_bvh_to_workflow",
        category: Category::Mesh3d,
        cost: CostTier::High,
        async_job: true,
        tags: &["conversion", "bvh", "workflow", "legacy"],
    },
    // Craft apps, featured, beta
    ToolMeta {
        name: "artcraft_get_craft_apps_release_info",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["craft", "apps", "release", "version"],
    },
    ToolMeta {
        name: "artcraft_list_featured_media",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["featured", "media", "list", "gallery"],
    },
    ToolMeta {
        name: "artcraft_search_featured_media",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["featured", "media", "search"],
    },
    ToolMeta {
        name: "artcraft_is_featured_item",
        category: Category::Community,
        cost: CostTier::Free,
        async_job: false,
        tags: &["featured", "check"],
    },
    ToolMeta {
        name: "artcraft_list_beta_keys",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["beta", "keys", "list"],
    },
    ToolMeta {
        name: "artcraft_redeem_beta_key",
        category: Category::Account,
        cost: CostTier::Free,
        async_job: false,
        tags: &["beta", "keys", "redeem"],
    },
];

/// Hot tools exposed directly in progressive mode.
pub const HOT_TOOLS: &[&str] = &[
    "artcraft_get_credits",
    "artcraft_estimate_cost",
    "artcraft_generate_image",
    "artcraft_generate_video",
    "artcraft_generate_3d_object",
    "artcraft_generate_audio",
    "artcraft_get_job_status",
    "artcraft_list_jobs",
    "artcraft_download_media_file",
];

/// Meta-tools always exposed in progressive/codemode modes.
pub const META_TOOLS: &[&str] = &[
    "artcraft_search_tools",
    "artcraft_describe_tool",
    "artcraft_call_deferred_tool",
];

pub fn meta_for(name: &str) -> Option<&'static ToolMeta> {
    REGISTRY.iter().find(|m| m.name == name)
}
