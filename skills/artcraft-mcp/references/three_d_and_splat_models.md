# Supported 3D and Gaussian Splat Models

The ArtCraft MCP server supports generating 3D assets and Gaussian Splats from images and text.

---

## 1. 3D Mesh Object Generation (`artcraft_generate_3d_object`)
Converts static 2D images or multi-view captures into fully realized 3D mesh assets.

### Available Models & Versions:
* **Hunyuan 3D**: `hunyuan_3d_3` (Default), `hunyuan_3d_3_sketch`, `hunyuan_3d_3p1_pro`, `hunyuan_3d_3p1_rapid`, `hunyuan_3d_3p1_part` (FBX input mesh), `hunyuan_3d_3p1_topology` (GLB/OBJ input mesh), and legacy `hunyuan_3d_2p0` / `hunyuan_3d_2p1`.
* **Tripo3D**: `tripo3d_h3p1` (text, single-image, or multi-view image input).
* **Meshy**: `meshy_v6` (text or single-image input).
* **Hyper3D Rodin**: `rodin_2p5_fast` (text or one or more images).

### Parameters:
* **`model`** (Optional): One of the model ids above. Defaults to `hunyuan_3d_3`.
* **`prompt`**: Text prompt for text-to-3D.
* **`image_media_token`**: Primary input image (upload via `artcraft_upload_image`). Extra images go in `reference_image_media_tokens`.
* **Multi-View Reference Tokens** (Optional): `front_image_media_token`, `back_image_media_token`, `left_image_media_token`, `right_image_media_token` for full 360° geometry consistency.
* **`input_mesh_media_file_token`**: Input mesh for `hunyuan_3d_3p1_part` and `hunyuan_3d_3p1_topology`.
* **Retopology & Materials** (Optional):
  * `output_type`: `normal`, `low_poly`, or `geometry`.
  * `polygon_type`: `triangle` or `quad`.
  * `target_face_count`: Target polygon count (e.g. 10000, 50000).
  * `enable_pbr`: Generate physically-based rendering roughness/metallic maps.
  * `enable_texture`: Texture synthesis toggle.
  * `texture_quality` / `geometry_quality`: `standard` or `detailed` (detailed costs extra on Tripo3D).

---

## 2. WorldLabs Marble Gaussian Splatting (`artcraft_generate_splat`)
Generates high-fidelity interactive 3D radiance fields (Gaussian Splats) from text descriptions, reference images, or both.

### Available Models & Versions:
* **`marble_0p1_mini` / `mini`**: Fast prototyping of scene layouts.
* **`marble_0p1_plus` / `plus`**: Standard production-grade radiance fields.
* **`marble_1p0` / `marble_1p0_draft`**: WorldLabs Marble 1.0 and its cheaper draft tier.
* **`marble_1p1` / `marble_1p1_plus`**: Next-gen WorldLabs Marble splatting models with high spatial depth and complex lighting (`marble_1p1_plus` is the default).
* **`triposplat`**: Tripo's Gaussian splat model.

### Parameters:
* **`image_media_token`**: Reference image to seed scene layout; extra views go in `reference_image_media_tokens`.
* **`prompt`**: Natural language scene description.
* **`is_panoramic`**: Set to `true` for 360° equirectangular environment splats.
* **`reference_video_media_token`**: Optional reference video.
* **`disable_recaption`**: Skip automatic prompt recaptioning.

### Pre-generation Cost Check:
* Always run `artcraft_estimate_splat_cost` with the target `model` (e.g. `marble_0p1_plus`, `marble_1p1`) to report exact credit costs before execution.
