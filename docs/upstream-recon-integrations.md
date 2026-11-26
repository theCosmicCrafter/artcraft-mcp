# Upstream recon: `craft-ai-integrations` and ArtCraft auth

Read-only reconnaissance for the ArtCraft MCP rebuild. Sources are the local upstream clones; no upstream files were modified.

Upstream revisions inspected:

- `storytold/craft-ai-integrations` `a822ad5b4db2c0dcd7a43bb62d6d75251fd3f8b5` (2026-11-20, `main`)
- `storytold/artcraft-services` `db9d9cde371a95bc4d9ba0c8dad0236cccc33879` (2026-11-26, `main`)

## Executive verdict

`craft-ai-integrations` is **not** the maintained general-purpose client library for the ArtCraft account API. It is a workspace of provider-neutral Craft AI contracts, provider adapters, secret stores, local model engines, shared egui widgets, and a separate Craft-app MCP gateway. Its ArtCraft crate, `craft-ai-artcraft`, is explicitly an **ArtCraft Omni API-key image/video provider**, “separate from account/subscription sign-in” (`crates/api/ai-artcraft/src/lib.rs:1`).

Do **not** migrate artcraft-mcp's client dependency to `craft-ai-integrations`. It covers only a small Omni generation slice and none of the account surface that drives most MCP tools. The practical AD-1 decision is:

1. keep hand-written route wrappers in `artcraft-mcp` for full control of auth, multipart upload, paging, and missing endpoint families;
2. use upstream `artcraft_api_defs` wire types where they are current, but re-pin them against the canonical backend source (`artcraft-services`) after verifying parity with the `storytold/artcraft` mirror currently referenced by `rust/artcraft_mcp_cli/Cargo.toml`;
3. treat `artcraft-services/crates/api_clients/artcraft/artcraft_client` as a useful reference and partial helper library, not as the complete client: it is unpublished, incomplete, and cookie-only in its shared request helpers;
4. borrow safety/design ideas from `craft-ai-integrations` (strict destination allowlists, bounded responses, no redirects with credentials, redacted secrets, job state machine, idempotency) without adopting its provider abstraction wholesale.

## `craft-ai-integrations`: what the repository is

The README describes it as the runtime half of the Craft AI layer: provider adapters, secret stores, the app-facing client, local runtime setup, and reusable egui widgets (`README.md:11-17`). It is a suite-wide AI abstraction layer, not an ArtCraft account API SDK.

Important crates (`README.md:22-40`, workspace members in `Cargo.toml:1-3`):

- `craft-ai-core` — serde-only data model and provider trait; wasm-compatible.
- `craft-ai-native` — OS keychain, encrypted-file secrets, environment secrets, shared store files.
- `craft-ai-client` — app-facing `AiClient`, model/tool binding, consent, planning, jobs, provider factory.
- `craft-ai-features` — tool catalogue and local-model requirements.
- `craft-ai-openai`, `craft-ai-bfl`, `craft-ai-gemini`, `craft-ai-vertex`, `craft-ai-seedance`, `craft-ai-artcraft` — hosted provider adapters.
- `craft-ai-comfyui`, `craft-ai-engine*` — local execution engines.
- `craft-ai-egui` — shared widgets.
- `craft-ai-agents`, `craft-ai-mcp` — launcher-managed agent enablement and a gateway to each Craft app's own `<app>-cli mcp`.

`craft-ai-artcraft`'s own package description is decisive: “ArtCraft Omni API-key image and video provider for Craft AI” (`crates/api/ai-artcraft/Cargo.toml:1-4`).

## Actual ArtCraft endpoint coverage in `craft-ai-integrations`

The adapter implementation uses exactly these ArtCraft routes:

| Endpoint                                                     | Use                                                | Evidence |
|--------------------------------------------------------------|----------------------------------------------------|----------|
| `POST /v1/omni_api/upload/image`                             | private intermediate PNG upload                    | `crates/api/ai-artcraft/src/http.rs:67-105` |
| `POST /v1/omni_api/generate/image`                           | image generation / Nano Banana inpainting          | `crates/api/ai-artcraft/src/provider.rs:229-236` |
| `POST /v1/omni_api/generate/video`                           | video generation                                   | `crates/api/ai-artcraft/src/provider.rs:229-236` |
| `GET /v1/omni_api/job_status/job/{inference_job_token}`      | job polling                                        | `crates/api/ai-artcraft/src/provider.rs:305-307` |

Model coverage is also narrow (`crates/foundation/ai-core/src/draft/omni_models.rs:25-53`):

- `nano_banana`, `nano_banana_2`, `nano_banana_pro` — prompt-only image generation plus mask-guided `image.inpaint`.
- `seedance_2p0` — prompt-only video generation.

Everything is marked “unverified” in the model labels. `ArtCraftProvider::models()` always refuses because Omni has no documented free key test; models are unverified until a paid job runs (`crates/api/ai-artcraft/src/provider.rs:143-150`).

### Coverage against the MCP server's needed endpoint surface

| Surface needed by artcraft-mcp       | Covered by `craft-ai-artcraft`? | Notes |
|--------------------------------------|---------------------------------|-------|
| image/video generation               | Partial                         | Only Omni API image/video and a four-model subset; no per-model `/v1/generate/**` matrix. |
| media files                          | No                              | Only private image upload tokens for Omni inpainting; no list/get/delete/rename/search/folder operations. |
| voice/TTS                            | No                              | No voice or TTS adapter surface. |
| characters                           | No                              | None. |
| folders                              | No                              | None. |
| billing/credits/subscription         | No                              | No account, wallet, Stripe, or billing routes. |
| API keys / MCP sessions              | No                              | It consumes an existing API key; it cannot manage keys or sessions. |
| bookmarks/tags/comments/weights/social | No                            | None. |
| job control                          | Very partial                    | Polls Omni jobs; cancellation is explicitly unsupported because Omni has no documented cancellation route (`provider.rs:384-391`). |

The upstream status document states the crucial limitation directly: “Account or subscription access has no adapter: no official programmatic route is documented. An API key is not evidence that a subscription covers the requests.” (`docs/STATUS.md:17`)

## Authentication in `craft-ai-integrations`

### Generic Craft AI authentication model

`craft-ai-core` models a connection with `Auth::None`, `Auth::ApiKey { secret: SecretRef }`, or `Auth::Subscription { route, secret }` (`crates/foundation/ai-core/src/connection.rs:29-43`). The connection stores only a `SecretRef`; the secret itself lives outside the shared connection file (`connection.rs:1-7`).

Keys normally travel in headers, never URLs. ADR 0009 requires every adapter to send keys in a request header and to disable redirects for keyed requests (`docs/adr/0009-keys-travel-in-headers.md:10-16`). `Connection.key_header` can select a nonstandard header, but `authorization`, `cookie`, transport headers, proxy headers, and `sec-*` names are forbidden (`connection.rs:82-140`); absent `key_header` means `Authorization: Bearer`.

Secret resolution is environment → OS keychain → passphrase-encrypted file (`crates/foundation/ai-native/src/secrets/standard.rs:1-14`, `standard.rs:57-67`). Environment variables are named `CRAFT_AI_SECRET_` plus the uppercased secret reference (`secrets/env.rs:1-17`).

### ArtCraft Omni adapter authentication

`craft-ai-artcraft` sends:

```text
Authorization: Bearer <api-key>
```

on upload, generation, and polling requests (`crates/api/ai-artcraft/src/http.rs:81-86`, `http.rs:199-216`). It uses no cookies and no redirects (`http.rs:1`, `http.rs:14-22`). The base URL is restricted to the official `https://api.storyteller.ai` or loopback test servers (`crates/api/ai-artcraft/src/provider.rs:68-77`, `provider.rs:93-99`). Keys must be single-line and free of whitespace/control characters (`provider.rs:80-91`).

This is compatible with the backend's API-key shape, but it is only a consumer of an existing key. It does not implement the web-session cookie flow, API-key creation, MCP-session creation, refresh, or revocation.

## Versioning and release state

- Workspace version is `0.1.0` (`Cargo.toml:5-10`).
- Every workspace crate has `publish = false`; no crates.io release exists.
- No git tags were present in the inspected shallow clone.
- The adoption guide explicitly says the crates are **not on crates.io** and consumers must pin one revision of `main` for all crates (`docs/adopting/use-in-your-app.md:17-30`).
- The extension APIs are explicitly “still evolving” (`README.md:14-17`).
- The status document says hosted providers are mostly tested against scripted localhost servers; ArtCraft Omni has only limited live evidence (`README.md:73-91`, `docs/STATUS.md:6-29`, `docs/STATUS.md:54-71`).

The repository is active and maintained, but it is not a stable versioned SDK. Pinning it would mean tracking a moving internal platform abstraction.

## The real first-party ArtCraft client in `artcraft-services`

There is a separate `artcraft_client` crate at `artcraft-services/crates/api_clients/artcraft/artcraft_client`. It is much closer to artcraft-mcp's domain, but still incomplete.

Its own lib docs call it “An API client for Artcraft / Storyteller / FakeYou” (`artcraft_client/src/lib.rs:1`). It is version `0.0.1` and `publish = false` (`artcraft_client/Cargo.toml`).

Implemented families visible in `artcraft_client/src/endpoints/` include:

- many `/v1/generate/**` image, video, object, splat, edit, inpaint, angle, background-removal, and cost-estimate calls;
- `/v1/omni_gen/**` generation and model listing;
- `/v1/jobs/session`;
- some media upload/list/delete/project helpers;
- prompt creation;
- credits, subscriptions, Stripe checkout/customer-portal helpers;
- analytics and moderation helpers.

The shared `artcraft_api_defs` crate is broader than the client wrappers. Its top-level modules include `api_keys`, `characters`, `folders`, `mcp_sessions`, `omni_api`, `omni_gen`, `tags`, `generate`, `media_file`, `stripe_artcraft`, and more (`artcraft_api_defs/src/`). Generated path constants cover 99 routes, including roughly 50 generation routes, 13 media-file routes, 7 Stripe routes, 22 moderation routes, jobs, prompts, analytics, and web referrals. The constants are concentrated in `artcraft_api_defs/src/**`; examples include `generate/image/edit/*`, `generate/video/multi_function/*`, `media_file/list/*`, and `stripe_artcraft/*`.

However, `artcraft_client` does **not** currently wrap the full endpoint surface artcraft-mcp needs. Missing or thin areas include folders, characters, TTS/voice, API keys, MCP sessions, bookmarks, comments, tags, weights, and much of the social/media management surface. It also authenticates through cookies only:

- `StorytellerCredentialSet` holds optional `visitor` and `session` cookie values (`artcraft_client/src/credentials/storyteller_credential_set.rs:5-9`).
- `maybe_as_cookie_header()` serializes them as `visitor=<value>; session=<value>` (`storyteller_credential_set.rs:75-91`; tests at `111-139`).
- `basic_json_get_request` puts that serialized pair in a `Cookie` header and has no API-key/MCP-session mode (`artcraft_client/src/utils/basic_json_get_request.rs:28-36`).

Therefore even the real first-party client is not a drop-in migration target for a modern MCP server. Its wire types and endpoint constants are valuable; its request helpers are not sufficient.

## AD-1 recommendation

**Recommendation: hand-patch and own the route layer in artcraft-mcp, while selectively consuming upstream wire types.**

Do not migrate to `craft-ai-integrations`:

1. wrong abstraction: provider-neutral AI tool routing, not account API access;
2. wrong coverage: four Omni endpoints versus hundreds of account/API routes;
3. no account lifecycle: no folders, billing, media management, characters, voice, API-key or MCP-session management;
4. unpublished/internal API: `publish=false`, version `0.1.0`, evolving contracts;
5. additional dependency cost: adopting `craft-ai-client` pulls in consent/store/provider machinery that duplicates what artcraft-mcp already does.

Do not rely solely on `artcraft_client` either:

1. it is also unpublished and incomplete;
2. its helpers are cookie-only while backend auth now supports API keys and MCP sessions;
3. its implementation families lag the backend and do not cover the MCP expansion surface.

Preferred route-forward strategy:

1. use `artcraft-services` as the canonical source of truth for paths and wire shapes;
2. update/re-pin `artcraft_api_defs`, `enums`, and `tokens` deliberately, after checking whether the public `storytold/artcraft` mirror is byte-compatible with `artcraft-services` for those crates;
3. keep `artcraft-mcp`'s typed/raw request helpers, but centralize path constants and response types so route drift is compile-time visible where possible;
4. implement missing endpoint families directly in `src/tools/*.rs` from backend handlers;
5. borrow `craft-ai-integrations` safety behavior: strict base URL validation, no redirects carrying credentials, bounded response reads, redacted secrets, idempotency tokens, and an explicit job state machine.

# Appendix A: exact official web frontend auth mechanism

This is the decision-critical web/Desktop/browser auth evidence extracted from `artcraft-services`.

## Cookie names and values

The backend's real cookie names are **`session`** and **`visitor`**. They are not `artcraft_session` or `artcraft_avt`; those names are local credential-file/environment conventions used by artcraft-mcp, not wire cookie names.

- `SESSION_COOKIE_NAME = "session"` and `SESSION_HEADER_NAME = "session"` (`crates/lib/actix_artcraft/src/sessions/user_sessions/http_user_session_manager.rs:17-21`).
- `VISITOR_COOKIE_NAME = "visitor"` (`crates/lib/actix_artcraft/src/sessions/anonymous_visitor_tracking/avt_cookie_manager.rs:11`).

The `session` cookie value is a signed session payload/JWT, not the raw database session token. `HttpUserSessionManager::create_cookie` creates it with:

- `Secure` outside localhost/test domains;
- `SameSite::None` in production-like secure contexts, `SameSite::Lax` for localhost/test;
- permanent;
- `Path=/`;
- `HttpOnly` is commented out, so it is not set (`http_user_session_manager.rs:58-87`).

The `visitor` cookie is likewise a signed JWT payload containing an AVT token, with the same secure/SameSite/permanent/path behavior (`avt_cookie_manager.rs:31-60`).

## Browser fetch behavior

The official shared frontend API manager sends every normal JSON API call with:

```ts
headers: buildSessionHeaders({
  Accept: "application/json",
  "Content-Type": "application/json",
}),
credentials: "include",
```

Evidence: `frontend/libs/api/src/lib/ApiManager.ts:121-129`.

Multipart calls use `credentials: "include"` and `Accept: application/json`, but no explicit `Content-Type` because the browser sets the multipart boundary (`ApiManager.ts:151-169`).

The frontend does **not** normally use `Authorization: Bearer` for web-session API calls. Browser auth is cookie-first, with a custom signed-session header fallback.

## Signed-session header fallback

For browsers that block third-party cookies, the frontend stores the signed session JWT in localStorage under:

```text
artcraft_signed_session
```

Evidence: `frontend/libs/api/src/lib/ApiManager.ts:7-30`.

`buildSessionHeaders()` adds it as a header literally named **`session`** when present (`ApiManager.ts:32-39`). The backend accepts the same signed payload from either the `session` cookie or the `session` header (`http_user_session_manager.rs:143-163`; header test at `204-219`).

Important: this is a signed web-session JWT fallback, not an API key and not an MCP private session token.

## Login/signup/session flow

1. `POST /v1/login` with `{username_or_email,password}` (`frontend/libs/api/src/lib/UsersApi.ts:118-145`).
2. Backend validates the password, creates a database web session, builds the `session` cookie, and returns the same signed payload as JSON field `signed_session` (`crates/service/web/storyteller_web/src/http_server/endpoints/users/login_handler.rs:164-234`).
3. Frontend stores `response.signed_session` with `storeSignedSession()` (`UsersApi.ts:146-155`).
4. Signup (`/v1/create_account`) and Google SSO (`/v1/accounts/google_sso`) follow the same pattern (`UsersApi.ts:255-272`, `UsersApi.ts:326-350`).
5. Logout keeps the stored session in place until the server sees `/v1/logout`, then clears localStorage (`UsersApi.ts:164-184`).
6. `GET /v1/session` is the session probe (`UsersApi.ts:50-90`). The backend returns `logged_in` and user info; if no valid visitor cookie exists, it sets a new `visitor` cookie (`crates/service/web/storyteller_web/src/http_server/endpoints/users/session_info_handler.rs:109-130`).
7. `GET /v1/session_token` can return `maybe_signed_session` for header use (`crates/service/web/storyteller_web/src/http_server/endpoints/users/session_token_info_handler.rs:17-24`), but the handler itself warns that this is a “huge security vuln” and says not to use it without maintainer approval (`session_token_info_handler.rs:26-29`).

## Desktop/Tauri fetch behavior

The frontend uses `FetchProxy`. In a Tauri desktop app it calls `@tauri-apps/plugin-http`, which maintains its own cookie jar and avoids third-party-cookie blocking; in a browser it uses ordinary `fetch` (`frontend/libs/tauri-utils/src/lib/FetchProxy.ts:7-33`).

## Backend credential precedence and accepted `Authorization` forms

The modern backend supports three credential classes:

1. web session cookie/header (`session`);
2. ArtCraft API key (`artcraft_api_...`);
3. MCP private session token (`mcp_session_...`).

`Authorization` parsing accepts all of these forms (`crates/api_clients/artcraft/artcraft_api_keys/src/parse_from_header_value.rs:3-19`):

```text
Authorization: Bearer <secret>
Authorization: Key <secret>
Authorization: <secret>
```

The scheme is case-insensitive and whitespace-tolerant. `Basic`, `Digest`, and multi-token unknown schemes are rejected (`parse_from_header_value.rs:20-58`, tests `70-125`).

If an `Authorization` header is present, it strictly wins. A malformed or unknown credential is a 401; the backend does **not** fall back to cookies (`crates/service/web/storyteller_web/src/http_server/user_lookup/api_or_web_session/require_any_session_or_key.rs:74-100`). The token prefix selects the credential type: `mcp_session_` means MCP private session; anything else is looked up as an API key (`require_any_session_or_key.rs:102-145`).

There are two lookup helpers with different admission rules:

- `require_any_session_or_key` admits API key, MCP session, or web cookie (`require_any_session_or_key.rs:32-66`).
- `require_api_or_web_session` admits API key or web cookie, but not MCP sessions (`require_api_or_web_session.rs:26-67`).
- older handlers may still use `require_user_session`, which is web-session only (the `session` cookie or signed `session` header), not an API-key/MCP bearer credential.

This means an MCP private session is not universally equivalent to a web session; each endpoint's helper matters. Uploads use `read_upload_session`, which explicitly accepts API keys and MCP sessions via `require_any_session_or_key`, while preserving optional anonymous cookie-less uploads (`endpoints/media_files/upload/common_utils/read_upload_session.rs:18-31`, `59-87`).

## API keys

API keys have the prefix `artcraft_api_` plus 40 lowercase Crockford characters, total length 53, about 200 bits of entropy (`crates/service/web/storyteller_web/src/util/generate_api_key.rs:5-25`). They are redacted after the first 20 characters in `Debug`/`Display` (`crates/api_clients/artcraft/artcraft_api_keys/src/lib.rs:7-17`, `46-66`).

`POST /v1/api_keys/create` creates a key, but it uses `require_user_session`: creation requires a web session, not an API key or MCP session (`crates/service/web/storyteller_web/src/http_server/endpoints/api_keys/create_api_key_handler.rs:35-46`). The full secret is returned exactly once (`create_api_key_handler.rs:70-92`; wire type at `crates/api_clients/artcraft/artcraft_api_defs/src/api_keys/create_api_key.rs:17-28`).

## MCP sessions

`POST /v1/mcp/session/create` is authenticated by API key or web session (`create_mcp_session_handler.rs:22-24`, `47-52`). It accepts optional client metadata (`maybe_mcp_client_name`, `maybe_mcp_client_version`, `maybe_mcp_client_vendor`) and returns, exactly once:

```json
{
  "success": true,
  "private_session_token": "mcp_session_...",
  "private_refresh_token": "mcp_refresh_..."
}
```

Wire shape: `crates/api_clients/artcraft/artcraft_api_defs/src/mcp_sessions/create_mcp_session.rs:7-34`.

Fresh/refreshed MCP sessions live 14 days (`crates/schema/database/mysql_queries/src/queries/mcp_sessions/insert_mcp_session.rs:10-12`, `34-38`). The private session token uses prefix `mcp_session_` and 64 Crockford characters (`crates/schema/public/tokens/src/tokens/mcp_session_private.rs:9-22`; prefix mapping in `crates/schema/public/tokens/src/prefixes.rs:130-132`).

Use the private session token as a normal bearer credential:

```text
Authorization: Bearer mcp_session_<secret>
```

Refresh is deliberately non-idempotent:

- `POST /v1/mcp/session/refresh`
- header: `Authorization: Bearer <private_session_token>`
- body: `{ "private_refresh_token": "<current refresh token>" }`
- response contains a new `private_refresh_token`; the old one is invalidated (`crates/service/web/storyteller_web/src/http_server/endpoints/mcp_sessions/refresh_mcp_session_handler.rs:20-31`, `38-81`).

`POST /v1/mcp/session/revoke` terminates the calling session using only its private credential. Dashboard management uses list/delete routes with API key or web session; list responses expose only the non-secret management token, never the private session token (`crates/api_clients/artcraft/artcraft_api_defs/src/mcp_sessions/common.rs:7-42`).

## CORS note

Production CORS supports credentials and allows the custom `session` header plus `x-artcraft-version`; it does not list `Authorization` in `allowed_headers` (`crates/lib/actix_cors_configs/src/cors.rs:71-90`). That is consistent with the browser frontend being cookie/session-header based. Native MCP clients do not use browser CORS, so this does not prevent them from sending `Authorization`.

## Auth recommendation for the parallel rebuild

Preferred credential order for artcraft-mcp:

1. `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN` → send `Authorization: Bearer <token>`, refresh through the rotating refresh token before day 14.
2. `ARTCRAFT_API_KEY` → send `Authorization: Bearer <key>`.
3. Legacy desktop/web compatibility → send cookie header with wire names `session=<signed JWT>` and `visitor=<signed JWT>`.

Do not mix the modes. If `Authorization` is present, upstream treats it as authoritative and ignores cookies.

Do not interpret local `ARTCRAFT_SESSION` / `ARTCRAFT_AVT` values as wire cookie names. They are only local labels; the actual cookie names are `session` and `visitor`, and the values are signed JWT payloads.

Avoid `/v1/session_token` as a general integration mechanism. The backend warns against it. Use it only as a last-resort migration bridge with explicit user consent and careful secret handling.

# Appendix B: audio upload formats

The backend audio upload handler is `POST /v1/media_files/upload/audio` in:

`crates/service/web/storyteller_web/src/http_server/endpoints/media_files/upload/upload_audio_media_file_handler.rs`

Multipart form fields (`upload_audio_media_file_handler.rs:34-66`):

- `uuid_idempotency_token` — required UUID-ish idempotency token, max 2 KiB;
- `file` — required file, max 512 MiB;
- `maybe_title` — optional;
- `maybe_visibility` — optional;
- `maybe_generation_provider` — optional.

Accepted MIME types, detected from file bytes rather than trusting the client filename or part content type (`upload_audio_media_file_handler.rs:75-91`, detection at `190-205`):

```text
audio/aac
audio/m4a
audio/mpeg
audio/ogg
audio/opus
audio/x-flac
audio/x-wav
audio/mp4
video/mp4
video/webm
```

Practical mapping: MP3 (`audio/mpeg`) and WAV (`audio/x-wav`) are explicitly accepted when byte sniffing identifies them correctly. MP4/WebM containers are also accepted even though they are video types.

Additional behavior:

- upload auth accepts API key/MCP bearer credentials or an optional web-session cookie; anonymous uploads are still allowed (`read_upload_session.rs:18-31`, `59-87`);
- bad header credentials are a 401, never anonymous fallthrough (`read_upload_session.rs:20-23`);
- unsupported sniffed type returns 400 `unpermitted mime type: ...`, not 415 (`upload_audio_media_file_handler.rs:197-205`; error mapping in `upload_error.rs:20-31`);
- duration probing is skipped for `audio/mp4`, `video/mp4`, and `audio/opus`; other accepted formats must decode successfully or the upload fails `could not decode file` (`upload_audio_media_file_handler.rs:207-257`);
- stored media class is `Audio`, media type derives from MIME type or filename with coarse `Audio` fallback (`upload_audio_media_file_handler.rs:317-345`).

The current MCP multipart sender does not set a per-part MIME type, only a filename (`rust/artcraft_mcp_cli/src/tools/http.rs:81-93`). That should normally be acceptable because the backend sniffs bytes, but a malformed/nonstandard MP3 or WAV can still fail byte detection or audio decoding. A 415 observed by the harness is therefore unlikely to be caused by the backend's allowlist alone; inspect the exact file bytes, multipart framing, and whether the endpoint or proxy rejected the media type before the handler.
