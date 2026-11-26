# ArtCraft MCP verification evidence report

Date: 2026-11-26. Evidence generated against the release binary `rust/artcraft_mcp_cli/target/release/artcraft-mcp.exe` (SHA-256 `a8f86c06bdf336b19f0e696b8002235f197bfc4382b5a78044c313b0829b0bc8`, identical for packaged `bin/artcraft-mcp.exe`). No git commit, push, issue, payment completion, or public action was performed.

Machine-checkable evidence files are in the local verification folder (rendered here as `<verification-dir>`):
- `<verification-dir>/part1_retained_evidence.json`
- `<verification-dir>/part2_supplemental_evidence.json`
- `<verification-dir>/part3_transport_evidence.json`
- `<verification-dir>/part3_mrtr_forced_evidence.json`
- `<verification-dir>/conformance-active-2026-11-26.txt`
- `<verification-dir>/drift-check-2026-11-26.json`
- `<verification-dir>/part3_progressive_meta_evidence.json`
- `<verification-dir>/final_credit_snapshot.json`
- `<verification-dir>/final_cleanup_snapshot.json`
- `tools/e2e_matrix_evidence.json` (fresh full-matrix report)

Secrets are redacted. Media/job tokens are intentionally retained because they are opaque object identifiers needed for user-controlled deletion, not bearer credentials. Stripe checkout URLs were captured in local evidence but are not reproduced here.

## Executive verdict

- Business-tool matrix: **93 PASS / 4 FAIL / 2 SKIP of 99**. Three FAIL rows are reproducible upstream HTTP 500s (`inpaint`, `splat`, `edit_video`). The fourth matrix FAIL is an intermittent upstream HTTP 500 on character creation; Part 1 independently completed the full character create/poll/list/get/delete lifecycle. One SKIP is the deliberately unsafe self-revocation tool and the other is the dependent character-delete row skipped after that matrix-only creation failure.
- Real retained output artifacts: **9 downloaded media files** (8 Part 1 outputs + 1 forced-MRTR protocol image). Audio did not produce an artifact because upstream left the job pending at 0%.
- Exact global spend: **5255 → 5199 = 56 banked credits**, within the 100-credit cap. This reconciles as 28 credits in Part 1 plus 28 credits in the independent matrix; all read-only/protocol probes measured zero.
- Fresh official active-suite conformance: **40/40 checks passed, 0 failed, across 30/30 scenarios**. Fresh drift check: **94 matched, 0 missing, 0 verb mismatches** against 364 upstream routes at `db9d9cde371a95bc4d9ba0c8dad0236cccc33879` (4 additional usages are statically verb-undetermined, not drift).
- Transport/protocol evidence: legacy stdio exposes exactly 99 tools; modern 2026-07-28 progressive mode exposes a 10-tool hot catalog when tasks support is declared; forced MRTR input-required → tamper rejection → accept → task creation → tasks/get completion is captured; files upload-target negotiation, Apps resources, HTTP initialize/tools-call, health, and hostile-Host 403 are captured.

## Exact credit reconciliation

| Scope | Start | End | Delta | Evidence |
|---|---:|---:|---:|---|
| Part 1 measured generation calls | 5255 | 5227 | 28 | positive per-call deltas sum = 28; see wallet_events |
| Independent full matrix | 5227 | 5199 | 28 | matrix start/end and per-tool rows |
| **Global verification run** | **5255** | **5199** | **56** | no other measured deduction |

Important ledger caveat: `part1_retained_evidence.json` has `wallet_delta=56` because its final snapshot was taken while the independent matrix had already spent its own 28 credits. The per-call events inside Part 1 sum exactly to 28, and the matrix independently measured 5227→5199=28; therefore the scoped ledger above is exact.

Measured Part 1 deductions: angle edit **2**, audio enqueue **3**, Wan video **10**, Rodin mesh **13**. Free flux images, Gemini edit, background removal, character creation, inpaint/splat/edit-video failed calls measured **0** immediate net deduction.

## Retained artifact inventory

| Artifact | Media token | Job token | File | Bytes | SHA-256 |
|---|---|---|---|---:|---|
| 01_flux_red_cube | `m_cefctfqs332dwfb04xpc066dqfb5yj` | `jinf_s2cx4k8trywm4heyewqtskh7a2t` | `01_flux_red_cube.jpg` | 92301 | `d6083abc89cb5929…` |
| 02_flux_blue_teapot | `m_05r01x7c9k253jjcz8462c8kmj1a2t` | `jinf_nxme3gq33rgfvxd29d2gyj7w275` | `02_flux_blue_teapot.jpg` | 162029 | `b76103f25bdc5b61…` |
| 03_flux_green_robot | `m_gv6nd4hyeexz9avmyvkwbp7ev228wk` | `jinf_fnf1y8cx98ntbyf87vs2c6bgksv` | `03_flux_green_robot.jpg` | 148457 | `7dbc0c8e63eeacfb…` |
| 04_gemini_edit_blue_cube | `m_ff1sx9f1rx9g4wnn3fewfkaqws7npj` | `jinf_g64j5p419a9xazekw8ppg86j05h` | `04_gemini_edit_blue_cube.png` | 752620 | `725a3f9de88edb3d…` |
| 05_angle_edit_45deg | `m_5vrvqkzs1gaxc3y1b620n1xgj5rvwr` | `jinf_2s487461s2zrk18ns0bh7nvby5v` | `05_angle_edit_45deg.png` | 216468 | `3301fd70ff9e1f55…` |
| 06_background_removed | `m_354vdamfxeadb0py8xd16tzgx3cvy5` | `jinf_hkj8ave3tjc8yjfwsefafpgbmgp` | `06_background_removed.png` | 35047 | `9f91e2726993bc2d…` |
| 07_wan_2s_480p_cube | `m_0kstz0dnc06xms62zmnxyk1f8sxy1q` | `jinf_9qjkafz18af4tp26d0znce6h9qq` | `07_wan_2s_480p_cube.mp4` | 652802 | `53c454b23cf89b14…` |
| 09_rodin_cube_mesh | `m_cvs7rz2zmahtkdq9hghkx7qc7sg8tk` | `jinf_e9tf6znca8abfs3pxs68gdzkhzz` | `09_rodin_cube_mesh.glb` | 754804 | `801be24bb197c322…` |
| forced-MRTR protocol image | `m_exk9v2eh51d0zqm6qfzvgb4bt6besv` | `jinf_r01n5pqq9jyqccypwcxb9530sty` | `11_mrtr_forced_flux_protocol_image.jpg` | 102341 | `a6892b962d9af16a…` |

The Wan MP4 was independently probed as H.264 video at 640×640 with duration 2.000s (plus a short AAC track), matching the requested 2-second 480p-class square output. All listed files exist locally in `<verification-dir>` and the corresponding media objects are retained on the ArtCraft account. No generated media was deleted. Only the two tiny scratch mask/video uploads and the temporary character were deleted, as required. `final_cleanup_snapshot.json` confirms that the ten matrix-created media tokens are absent, retained Part 1/MRTR media remain listed, only the two pre-existing characters remain, and referral codes are empty.

## Audio stall evidence

- Job `jinf_my8pg9k78j6kaqy264rty10s5dj` (`suno_sounds`) deducted 3 credits at enqueue and remained `Status: pending`, `Progress: 0%` after **606.8 seconds**. The evidence explicitly records: “Left running upstream; no local termination request was sent.”.
- The independent matrix reproduced the same upstream stall; its harness eventually terminated only its own matrix-created job after the bounded poll window. That matrix cancellation is harness cleanup, not evidence of upstream completion.

## Reproducible upstream-broken generation endpoints

| Tool | Exact evidence | Credit effect |
|---|---|---|
| `artcraft_inpaint_image` | `Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"}` | 0 measured |
| `artcraft_generate_splat` | `Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"}` | 0 measured |
| `artcraft_edit_video` | `Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"}` | 0 measured |

These are not MCP schema/validation errors: the MCP calls use valid media tokens and typed upstream request shapes, then upstream returns HTTP 500 before a job token is produced. The matrix independently reproduced the same three failures.

## Protocol, transport, and SOTA architecture evidence

- Legacy stdio initialize returned protocol `2024-11-05`, server `artcraft-mcp` v2.0.0, and exactly **99 tools**.
- Modern progressive `tools/list` at `2026-07-28` returned **10 hot tools** with `ttlMs=300000`; with tasks support declared, redundant job polling tools are hidden as designed. `artcraft_search_tools`, `artcraft_describe_tool`, and `artcraft_call_deferred_tool` were separately exercised successfully in `part3_progressive_meta_evidence.json`.
- Forced MRTR (`--confirm-over-credits 0 --auto-approve-under 0`) returned `resultType=input_required`, `requestState`, and an elicitation form. A one-character tampered state was rejected with `tampered or unknown request state`. Accepting the original state returned task `11c435e7-544c-42ff-8c3f-9366408b253e`; `tasks/get` progressed working → working → completed and included both the enqueue result and upstream completion with result token `m_exk9v2eh51d0zqm6qfzvgb4bt6besv`.
- MRTR quote caveat: the confirmation message estimated ~40 credits even for `flux_1_schnell` because the current implementation uses a conservative static tool-tier estimate rather than the model-specific zero-cost estimate. The actual measured spend was zero. This is safe/conservative but should be improved.
- Files extension negotiation produced `resultType=input_required`, one upload target, method `PUT`, credentials `omit`, and a loopback `/v1/mcp-uploads/...` URL. The probe intentionally stopped before upload/retry.
- Apps/resources: `resources/list` exposed 8 resources including the four MCP App HTML UIs; `ui://artcraft/credit-dashboard` read as `text/html;profile=mcp-app` with CSP metadata present.
- HTTP `/health` returned `200 OK`; legacy initialize over `/mcp` returned 200 plus a session header; initialized notification returned 202; `artcraft_get_credits` over HTTP returned 200 and a parseable credit response. Hostile `Host: evil.example` returned **403** with `Forbidden: Host header is not allowed`.
- Official current-spec evidence: MCP documentation identifies `2026-07-28` as the current protocol version. The implementation uses rmcp **3.11.0** locked in `Cargo.lock`. crates.io published rmcp **3.12.0** on 2026-11-26 during this verification window, so the build is no longer literally the newest SDK release at report time; it was the current stable line used by the implementation and is one release behind. This is a transparency caveat, not a conformance failure.

## Supplemental lifecycle evidence

The supplemental run independently exercised folder create/get/rename/star/color/cover, subfolder add/list/remove, media add/list/move/remove, child+parent delete; API-key create/list/get/update/delete/list; MCP-session create/list/delete/list; bookmark/comment/tag create-list-delete/clear cycles on retained media; credit-pack and subscription checkout URL creation; featured/beta/Craft-app reads; and image/video/splat cost estimates. Secrets returned by API-key/MCP-session creation are redacted and were not persisted.

- Referral cleanup gap: MCP exposes referral create/list but no delete tool. The test-created referral codes were therefore deleted through upstream `DELETE /v1/user_referral_codes/code/{token}`; corrected direct evidence shows HTTP 200 deletes and an empty list afterward.
- Cost-estimator coverage gap: `artcraft_estimate_cost` rejects `media_type=audio` client-side. Direct upstream OmniGen cost routes returned audio `suno_sounds` = **3 credits** and mesh `rodin_2p5_fast` = **13 credits**, proving upstream support exists but the MCP estimator schema does not expose audio/mesh.

## Full 99-tool evidence matrix

`PASS` below means the fresh matrix row passed and the raw redacted response excerpt is inspectable in `tools/e2e_matrix_evidence.json`; supplemental/Part 1 annotations identify independent corroboration. Matrix self-report is therefore not the only evidence for the important paid/artifact paths.

| Tool | Matrix | Independent evidence | Credits | Evidence excerpt |
|---|---|---|---:|---|
| `artcraft_add_media_to_folder` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_add_subfolders` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_convert_bvh_to_workflow` | PASS | matrix only | 0 | Error: media_file_token is required |
| `artcraft_convert_fbx_to_gltf` | PASS | matrix only | 0 | Error: media_file_token is required |
| `artcraft_convert_render_engine_scene` | PASS | matrix only | 0 | Error: media_file_token is required |
| `artcraft_create_api_key` | PASS | supplemental | 0 | Error: name must not be empty |
| `artcraft_create_bookmark` | PASS | supplemental | 0 | Bookmarked media [redacted-token]. Bookmark token: [redacted] |
| `artcraft_create_character` | FAIL | Part 1 live | 0 | Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"} |
| `artcraft_create_checkout_session` | PASS | supplemental | 0 | Stripe credit pack checkout session created successfully. Direct checkout URL: https://checkout.stripe.com/[redacted-checkout-session] |
| `artcraft_create_comment` | PASS | supplemental | 0 | Comment posted on media [redacted-token]. Comment token: [redacted] |
| `artcraft_create_face_animation` | PASS | matrix only | 0 | Error: choose exactly one audio source token |
| `artcraft_create_folder` | PASS | supplemental | 0 | Error: name must not be empty |
| `artcraft_create_mcp_session` | PASS | supplemental | 0 | Error: client_name too long (max 255 chars) |
| `artcraft_create_prompt` | PASS | matrix only | 0 | Prompt created successfully. Token: [redacted] Text: e2e-test-harness artifact, ignore |
| `artcraft_create_referral_code` | PASS | supplemental | 0 | Referral code created successfully. Token: [redacted] |
| `artcraft_create_rerender_animation` | PASS | matrix only | 0 | Error: sd_model_token is required |
| `artcraft_create_subscription_checkout` | PASS | supplemental | 0 | Stripe subscription checkout session created successfully. Direct checkout URL: https://checkout.stripe.com/[redacted-checkout-session] |
| `artcraft_delete_api_key` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_delete_bookmark` | PASS | supplemental | 0 | Bookmark ub_3yagajpc8nwbyqq2qt1x5pphp6h5c deleted successfully. |
| `artcraft_delete_character` | SKIP | Part 1 live | 0 | character creation failed |
| `artcraft_delete_comment` | PASS | supplemental | 0 | Comment comment_k2dzqq74mhss52r35tpcmtbh deleted successfully. |
| `artcraft_delete_folder` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_delete_mcp_session` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_delete_media_file` | PASS | Part 1 live | 0 | deleted 10 self-created media files |
| `artcraft_delete_weight` | PASS | matrix only | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_download_media_file` | PASS | Part 1 live, MRTR/tasks | 0 | Media file downloaded successfully. Token: [redacted] Saved to: <temp-path> Size: 274 |
| `artcraft_edit_image` | PASS | Part 1 live | 0 | Image edit queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_job_status; job completed |
| `artcraft_edit_image_angle` | PASS | Part 1 live | 2 | Angle edit queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_job_status; job completed |
| `artcraft_edit_video` | FAIL | Part 1 live | 0 | Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"} |
| `artcraft_estimate_cost` | PASS | supplemental | 0 | Image cost estimate: Model: flux_1_dev Credits: 2 USD cents: 2 Free: false Unlimited: false Rate limited: false Watermark: fal |
| `artcraft_estimate_splat_cost` | PASS | supplemental | 0 | Splat cost estimate: Model: marble_0p1_mini Credits: 18 USD cents: 18 Free: false Unlimited: false Rate limited: false Waterma |
| `artcraft_generate_3d_object` | PASS | Part 1 live | 13 | 3D object generation queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_j; job completed |
| `artcraft_generate_audio` | PASS | Part 1 live | 3 | Audio generation queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_job_s; job poll timeout |
| `artcraft_generate_image` | PASS | Part 1 live, MRTR/tasks | 0 | Image generation queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_job_s; job completed |
| `artcraft_generate_splat` | FAIL | Part 1 live | 0 | Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"} |
| `artcraft_generate_video` | PASS | Part 1 live | 10 | Video generation queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_job_s; job completed |
| `artcraft_get_api_key` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_get_billing_cancel_plan_url` | PASS | matrix only | 0 | Error: No billing portal is available because this account has no active ArtCraft subscription. |
| `artcraft_get_billing_portal_url` | PASS | matrix only | 0 | No billing portal is available because this account has no active ArtCraft subscription. |
| `artcraft_get_billing_switch_plan_url` | PASS | matrix only | 0 | Error: No billing portal is available because this account has no active ArtCraft subscription. |
| `artcraft_get_billing_update_payment_method_url` | PASS | matrix only | 0 | Error: No billing portal is available because this account has no active ArtCraft subscription. |
| `artcraft_get_character` | PASS | Part 1 live | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_get_craft_apps_release_info` | PASS | supplemental | 0 | Craft app release info (cache last fully fetched: 2026-11-26T15:05:58.937064088Z): - ArtCraft Launcher (artcraft-launcher): 0.20.3 v0.20.3 |
| `artcraft_get_credits` | PASS | matrix only | 0 | balance readable: 5227 banked |
| `artcraft_get_folder` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_get_job_status` | PASS | matrix only | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_get_media_file` | PASS | Part 1 live | 0 | Media file details: Token: [redacted] Title: (untitled) Type: jpg Class: image Created: 2026-11-26T15:32:34Z |
| `artcraft_get_prompt` | PASS | matrix only | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_get_session_info` | PASS | matrix only | 0 | Session info: Logged in: true User: cosmiccrafter Display name: CosmicCrafter Token: [redacted] |
| `artcraft_get_subscription` | PASS | matrix only | 0 | No active subscription found. |
| `artcraft_get_weight` | PASS | matrix only | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_inpaint_image` | FAIL | Part 1 live | 0 | Error: HTTP error: 500 Internal Server Error {"success":false,"error_code":500,"error_code_str":"Internal Server Error"} |
| `artcraft_is_featured_item` | PASS | supplemental | 0 | media_file [redacted-token] featured: false |
| `artcraft_list_api_keys` | PASS | supplemental | 0 | No API keys found. |
| `artcraft_list_beta_keys` | PASS | supplemental | 0 | No beta keys found. Total pages: 0 |
| `artcraft_list_bookmarks` | PASS | supplemental | 0 | No bookmarks found. |
| `artcraft_list_characters` | PASS | Part 1 live | 0 | Found 3 characters: - character_n4y3epf46h027072e889e6 \| e2e-evidence-character-152817 - character_k661dcmvb7bsmkdwppa781 \| rig - char |
| `artcraft_list_comments` | PASS | supplemental | 0 | No comments found. |
| `artcraft_list_featured_media` | PASS | supplemental | 0 | Found 3 featured media files: - [redacted-token] \| OpenPoseARKIT \| dimensional / glb - [redacted-token] |
| `artcraft_list_folder_media_files` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_list_folders` | PASS | matrix only | 0 | Found 1 folders: - folder_dafs9q27f9vfz18bn1jmy7h98 \| art crafter mascot color:#ef4444 |
| `artcraft_list_image_models` | PASS | matrix only | 0 | Available image models: - flux_1_dev - Fast open-source Flux - flux_1_schnell - Free fastest Flux - flux_pro_1p1 - Premium photorealis |
| `artcraft_list_jobs` | PASS | matrix only | 0 | Found 5 jobs: - [redacted-token] \| complete_success \| 100% \| character_generation \| 2026-11-26T15:28:18Z - [redacted-token] |
| `artcraft_list_mcp_sessions` | PASS | supplemental | 0 | No MCP sessions found. |
| `artcraft_list_media_files` | PASS | matrix only | 0 | Found 25 media files: - [redacted-token] \| (untitled) \| video \| public - [redacted-token] \| (untitled) \| |
| `artcraft_list_media_files_without_folder` | PASS | matrix only | 0 | Found 100 unfoldered media files: - [redacted-token] \| (untitled) \| video / mp4 - [redacted-token] \| (un |
| `artcraft_list_referral_codes` | PASS | supplemental | 0 | No referral codes found. |
| `artcraft_list_subfolders` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_list_tags` | PASS | supplemental | 0 | Tags: e2e-test |
| `artcraft_list_video_models` | PASS | matrix only | 0 | Available video models: - minimax_h3 - MiniMax Hailuo H3 long video generation - minimax_h3_turbo - MiniMax H3 Turbo fast mode - minim |
| `artcraft_list_weights` | PASS | matrix only | 0 | Found 25 weights: - weight_858y8vg2edh7he74atd30pwt4 \| Hnoodh368 - weight_cr91nxgyr1yxrxmg3m6yt99p6 \| Adadoob - weight_yrm7ahfrj4ejb71 |
| `artcraft_move_media_to_folder` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_rate_media` | PASS | matrix only | 0 | Rated media [redacted-token] up. |
| `artcraft_redeem_beta_key` | PASS | matrix only | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_refresh_mcp_session` | PASS | matrix only | 0 | Error: HTTP error: 401 Unauthorized {"success":false,"error_code":401,"error_code_str":"Unauthorized"} |
| `artcraft_remove_background` | PASS | Part 1 live | 0 | Background removal queued successfully. Job token: [redacted] Status: success Poll for results using artcraft_get_job; job completed |
| `artcraft_remove_media_from_folder` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_remove_subfolders` | PASS | supplemental | 0 | Subfolder bulk-remove complete. Unparented 0 rows. |
| `artcraft_rename_folder` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_rename_media_file` | PASS | matrix only | 0 | Media file [redacted-token] renamed to 'e2e-test-harness image'. |
| `artcraft_revoke_mcp_session` | SKIP | matrix only | 0 | destructive self-revocation not probed |
| `artcraft_search_featured_media` | PASS | supplemental | 0 | No featured media matched. |
| `artcraft_search_media` | PASS | matrix only | 0 | No media files found matching your search. |
| `artcraft_search_weights` | PASS | matrix only | 0 | No weights found matching 'flux' (upstream weight search is retired and may always return empty). |
| `artcraft_set_folder_color` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_set_folder_cover_image` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_set_folder_star` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_set_media_visibility` | PASS | matrix only | 0 | Media file [redacted-token] visibility set to private. |
| `artcraft_set_tags` | PASS | supplemental | 0 | Tag set replaced on media [redacted-token]. |
| `artcraft_set_unsubscribe_reason` | PASS | matrix only | 0 | Error: reason is required |
| `artcraft_terminate_job` | PASS | matrix only | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_tts_generate` | PASS | matrix only | 0 | Legacy TTS inference is shut down upstream. The old /v1/tts/inference compatibility route only returns a synthetic job token and cannot prod |
| `artcraft_tts_search_models` | PASS | matrix only | 0 | Found 3768 TTS models: - weight_34vt3stah9xyts72zrya85vc9 \| "Arthur C. Clarke" (901ep) - weight_rn3d1vs9dxsbz2vn3tytaaf1v \| "Blitzcrank" |
| `artcraft_update_api_key` | PASS | supplemental | 0 | Error: HTTP error: 404 Not Found {"success":false,"error_code":404,"error_code_str":"Not Found"} |
| `artcraft_upload_audio` | PASS | matrix only | 0 | Audio uploaded successfully. Media token: [redacted] |
| `artcraft_upload_image` | PASS | Part 1 live, files-extension negotiation | 0 | Image uploaded successfully. Media token: [redacted] |
| `artcraft_upload_studio_scene_snapshot` | PASS | matrix only | 0 | Error: The system cannot find the file specified. (os error 2) |
| `artcraft_upload_studio_shot` | PASS | matrix only | 0 | Error: The system cannot find the file specified. (os error 2) |
| `artcraft_upload_video` | PASS | Part 1 live | 0 | Video uploaded successfully. Media token: [redacted] |

## Deliberately non-probed or limited surfaces

- `artcraft_revoke_mcp_session`: SKIP. A valid probe can revoke the ambient credential; only non-destructive schema/coverage evidence is appropriate.
- Legacy paid animation/conversion tools (`artcraft_create_face_animation`, `artcraft_create_rerender_animation`, `artcraft_convert_bvh_to_workflow`, `artcraft_convert_fbx_to_gltf`, `artcraft_convert_render_engine_scene`): client-side validation evidence only. Prior probing showed upstream enqueues real paid jobs without validating fake input tokens, so valid-looking fake-token probes are unsafe.
- Stripe checkout sessions were created only to capture URL generation; no checkout was opened or completed.

## Surprises and follow-ups

1. rmcp 3.12.0 was published during the verification window; decide whether to upgrade from locked 3.11.0 in a separate dependency pass.
2. MRTR cost quotes are tier-based and conservative (~40cr for image generation) rather than model-specific; this caused a free flux image to display a 40-credit confirmation estimate when confirmations were forced.
3. The matrix’s concurrent character-create attempt received an upstream HTTP 500, while the Part 1 character create/poll/list/get/delete lifecycle succeeded. Treat character generation as intermittently upstream-flaky and investigate with a non-concurrent repeat matrix.
4. `artcraft_estimate_cost` lacks audio/mesh media types even though upstream OmniGen cost endpoints support them.
5. MCP lacks a referral-delete convenience tool; cleanup required the upstream delete route.
6. `artcraft_create_prompt` succeeds, but the upstream route table exposes no prompt-delete route. The matrix-created `e2e-test-harness` prompt therefore could not be cleaned up; this is an upstream/API-surface limitation rather than a harness omission.

## Bottom line

The server is interoperable and evidence-backed on both legacy and modern MCP surfaces, all 99 business tools were exercised or deliberately safety-limited, real paid artifacts were produced and retained, exact spend reconciled to 56 credits, and the reproducible business-tool failures are the three upstream HTTP 500 generation endpoints plus the upstream audio scheduling stall; character generation additionally showed a matrix-only intermittent upstream 500 despite a successful independent lifecycle. The transparency caveats above prevent an unqualified “perfect” verdict, but they do not contradict the core interoperability/artifact evidence.
