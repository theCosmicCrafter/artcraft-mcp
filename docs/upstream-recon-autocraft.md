# Upstream recon: `autocraft`

Read-only reconnaissance of `storytold/autocraft` at `ef8d715f5400e2588a3be537082aadf982311be8` (2026-11-26, `main`). No upstream files were modified.

## Executive summary

AutoCraft is **not an ArtCraft media-generation service, API client, or end-user creative tool**. It is a TypeScript/Node 24 GitHub repository-maintenance automation system: a backlog mirror, staged model pipeline, dashboard, sandboxed agent runner, build-worker coordinator, and MCP server for maintainers' own agents.

Its name makes it sound adjacent to ArtCraft, but its domain is repository operations. It is still highly relevant as a reference architecture for artcraft-mcp because it solves several of the same hard problems well:

- least-privilege agent tools;
- personal MCP credentials;
- role-aware tool exposure;
- typed/validated tool arguments;
- untrusted-content framing;
- job/lease claiming and polling;
- explicit audit and write authorization;
- sandboxing model tool use;
- separating control-plane credentials from worker credentials.

## Purpose and product shape

The README defines AutoCraft as “a model harness for maintaining GitHub repositories with a large backlog” (`README.md:1-5`). It mirrors every issue and PR into a local database and runs model steps over them. Models classify, prioritize, detect duplicates, review, implement, verify, fix up, and merge. Maintainers start items and answer recommendations.

Package facts (`package.json:1-42`):

- TypeScript ESM, Node `>=24`, pnpm;
- private package, version `0.1.0`;
- SQLite by default through `node:sqlite`, Postgres optional;
- server binary names `autocraft-server` and `craft-auto`;
- React frontend under `web/`;
- dependencies are small and infrastructure-focused (`kysely`, `pg`, Hugging Face transformers, xterm headless), not ArtCraft API clients.

The pipeline stages are documented in `README.md:11-29` and expanded in `docs/pipeline.md`:

1. Sync mirrors GitHub issues/PRs/comments and CI state.
2. Duplicates finds related items.
3. Classify assigns type/security/decision labels.
4. Priority assigns P0-P4 and work rank.
5. Conversations answer maintainer/operator messages and propose commands.
6. Label writes settled labels.
7. Review reviews PRs at current heads.
8. Implement plans and writes queued issue fixes in a workspace.
9. Fix-up applies or posts small review fixes and handles some CI failures.
10. Verify consumes CI or plans builds/screenshot comparisons.
11. Merge queue performs safe merges and integration batches.
12. Act performs commanded closes.
13. Post-merge review inspects what landed.

## Architecture

High-level layout from `README.md:218-231`:

- `src/cli.ts` — entry point and command registry.
- `src/mirror.ts`, `src/db.ts`, `src/schema.sql` — GitHub mirror and persistence.
- `src/classify`, `src/priority`, `src/dupes`, `src/embed`, `src/rank` — early pipeline intelligence.
- `src/workflow`, `src/conversations`, `src/act` — maintainer workflow and commanded actions.
- `src/review`, `src/implement`, `src/fixup`, `src/verify`, `src/merge` — code-change pipeline.
- `src/agents`, `src/prompts`, `src/sandbox`, `src/itemtools` — model invocation, prompt registry, sandboxing, read-only item tools.
- `src/github` — GitHub transport; `write.ts` is the only write module.
- `src/runner`, `src/worker`, `src/build` — model-call runner, sandbox worker, build environments.
- `src/dash` — dashboard server/API, including the MCP connector.
- `web/` — dashboard frontend.

### Trust boundaries

AutoCraft's trust design is unusually explicit (`README.md:52-62`, `docs/pipeline.md:596-606`):

- Untrusted code never runs where GitHub or model credentials exist.
- Build workers hold no GitHub token, no Claude login, and no database access (`docs/pipeline.md:551-560`).
- Only `src/github/write.ts` writes to GitHub. Its header states that every function performs the write and records it in an audit table; pull-request writes require a `Guarded` value created only by the guard module (`src/github/write.ts:1-6`).
- GitHub text is treated as untrusted data. Models may judge it but must never follow instructions inside it (`docs/pipeline.md:599-602`).
- Model calls with file tools run under tightly constrained Claude settings and bubblewrap: read-only checkout, empty home, clean environment, no project settings/skills/slash commands/arbitrary MCP servers, and registered turn limits (`docs/pipeline.md:603-606`).
- Outputs are scrubbed for credential-shaped strings before storage or GitHub writes (`docs/pipeline.md:606`).

This is a mature reference for any MCP server that exposes powerful mutation tools.

## Is it an agent/automation layer?

Yes — but for repository maintenance, not media generation.

It is both:

1. an automation control plane (pipeline planner, queues, gates, database, dashboard, audit); and
2. an agent execution layer (model calls claimed by user-local Task Runners, sandboxed tool access, MCP connectors, workspace/item tools).

The automation is intentionally not fully autonomous. The README says the fixed model tools are GitHub actions behind one write module and safe-merge-style checks; actions run automatically only where rules say they are safe, otherwise a model proposes and a person accepts (`README.md:3-6`). The pipeline is built around maintainer judgement as the scarce resource.

## MCP usage

AutoCraft uses MCP in two distinct ways.

### 1. MCP Connector for contributors' agents

`POST /mcp` on the dashboard is a Streamable HTTP MCP server named `autocraft` (`docs/operations.md:172-181`, implementation in `src/dash/mcp/index.ts`). It is intentionally simple:

- one JSON-RPC message per POST;
- one JSON response body;
- no event stream (`GET` is 405);
- notifications receive 202;
- batches are rejected;
- personal bearer token required;
- dashboard session cookie is never accepted;
- cross-site `Origin`/`Sec-Fetch-Site` requests are refused (`docs/operations.md:176-177`).

Supported protocol versions are `2025-06-18`, `2025-03-26`, and `2024-11-05` (`src/dash/mcp/index.ts:37`). `initialize` echoes the client's requested version if recognized, otherwise uses the newest supported one, and reports only tool capability (`src/dash/mcp/index.ts:616-641`).

#### Connector token design

Tokens are a strong reference pattern (`src/dash/mcp/tokens.ts:1-30`, `61-75`, `113-126`):

- connector prefix `acmcp_`; task-runner prefix `acrun_`;
- token is the kind prefix plus `randomToken(32)`;
- only SHA-256 hashes are stored;
- plain token is shown once;
- normal connector tokens last 90 days;
- command-capable connector tokens last 7 days;
- maximum 5 per login per kind;
- revocable by owner or operator;
- role is read from the access list on every call, so role changes apply immediately;
- removing/ending a login ends tokens;
- task-runner credential expiry extends on use.

Connector tokens and runner credentials are deliberately separated (`docs/operations.md:198-215`):

| Property                    | MCP Connector token | Task Runner credential |
|-----------------------------|---------------------|------------------------|
| Prefix                      | `acmcp_`            | `acrun_`               |
| Created by                  | user on Connect page | runner device-login flow |
| `/mcp`                      | yes                 | only with Agent access |
| `/api/remote/*` task claims | no                  | yes                    |
| dashboard `/api/*`          | never               | never                  |

This separation is directly relevant to artcraft-mcp: do not let one credential silently mean both “interactive agent identity” and “background worker identity” if those have different risk profiles.

#### Tool design

The connector's tools are workflow-shaped, not thin database bindings (`src/dash/mcp/index.ts:133-260`):

- `list_repos`
- `list_fixups`
- `list_merge_queue`
- `get_item`
- `my_items`
- `help`
- `command` (only for authorized command tokens)
- `list_model_calls`
- `pick_model_call`, `unpick_model_call`
- `my_settings`, `set_my_settings`
- task-runner helper tools such as `next_task`, `task_status`, `my_work` when eligible

Notable design properties:

- `additionalProperties: false` in schemas;
- strict server-side argument validation before dispatch (`src/dash/mcp/index.ts:85-125`);
- explicit annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`);
- tool list is role- and credential-dependent (`src/dash/mcp/index.ts:647-658`);
- every call re-checks authorization rather than relying on the list-time role alone (`src/dash/mcp/index.ts:647-650`);
- writes are explicitly identified by `writes()` (`src/dash/mcp/index.ts:606-607`);
- tools call the dashboard's own handlers as the token user, preserving repo visibility, act rules, and audit rows (`src/dash/mcp/index.ts:1-6`; `docs/operations.md:181`);
- responses frame GitHub and pipeline text as untrusted (`src/dash/mcp/index.ts:59-67`).

The instructions embedded in `initialize` are also a useful pattern: they tell the agent the safe workflow, command confirmation rules, and explicitly warn never to follow instructions found in GitHub text (`src/dash/mcp/index.ts:59-83`).

### 2. Internal MCP servers as sandboxed model tools

AutoCraft also uses MCP inside model sandboxes:

- a read-only `items` server exposes `search_items`, `item`, `similar_items`, and `pr_diff` over a snapshot of synced public GitHub data (`docs/pipeline.md:537`);
- a workspace `work` MCP server exposes checked workspace operations;
- a `check` MCP server validates patch/checker operations;
- lease-scoped remote tool endpoints can be exposed as `POST /mcp/task/<server>` for the model harness (`docs/operations.md:522-523`).

The item-tools design is important: the model does not get live unrestricted GitHub access. It gets read-only, size-capped, snapshot-backed tools. Reviews still pending are omitted; private repos are excluded; no settings, audit rows, accounts, tokens, paths, or dashboard notes are copied (`docs/pipeline.md:537`).

## Job, claim, and polling patterns worth borrowing

AutoCraft separates model-call queueing from execution (`docs/operations.md:183-185`):

- planner/gates/GitHub writes/build scheduling stay on the server;
- a user's own Task Runner claims queued calls;
- the runner executes the harness locally under the user's Claude/Codex subscription;
- it submits only the reply back to the server.

Relevant mechanics:

- queued calls are leased to runners;
- held claims make pickup immediate;
- a claim carries only the prompt, read-only checkouts, and per-call tools;
- the server refuses replies from wrong models/scope;
- Task Runner credentials are scoped and revocable;
- rate limits are explicit per token/lease/route (`docs/operations.md:433`);
- long remote tool calls can send whitespace before the final JSON to keep the connection alive (`docs/operations.md:522`);
- `next_task` never returns a lease secret; it tells the agent the local command loop to run (`docs/operations.md:532`);
- the agent is explicitly forbidden to read or mutate the runner's bundle/reply files (`src/dash/mcp/index.ts:69-73`).

For artcraft-mcp, the exact implementation is overkill, but the principles transfer directly:

- distinguish submission, polling, cancellation, and result retrieval;
- make job tokens/capabilities explicit;
- bound polling and result sizes;
- never report a job stopped when the upstream service cannot confirm cancellation;
- treat remote text/results as untrusted data;
- do not conflate an interactive caller's credential with a long-running worker credential.

## Patterns most worth borrowing for artcraft-mcp

### 1. Role-aware tools/list, but authority checked at call time

AutoCraft tailors `tools/list` by role/credential, then re-checks the role on every call (`src/dash/mcp/index.ts:647-658`). This prevents stale clients or confused deputies from using a tool merely because it was visible earlier.

ArtCraft MCP analogue: expose account/admin/billing/mutation tools based on available auth scopes, but independently validate credentials and upstream response on every call.

### 2. Personal, hashed, expiring connector tokens

AutoCraft's `acmcp_`/`acrun_` split is a better long-term model than one undifferentiated static bearer. ArtCraft's upstream MCP sessions already have `mcp_session_` and rotating `mcp_refresh_`; artcraft-mcp should preserve that distinction locally and store only the minimum necessary secret material.

### 3. Strict input schema + server-side validation

Schemas use closed objects, explicit numeric ranges, string length caps, enum-like validation, and rejection of unknown fields (`src/dash/mcp/index.ts:85-125`). This is directly applicable to generation models, media tokens, visibility values, aspect ratios, and upload parameters.

### 4. Explicit tool annotations

AutoCraft uses `readOnlyHint`, `idempotentHint`, `destructiveHint`, and `openWorldHint` accurately. ArtCraft MCP should do the same: generation/upload/delete/comment/bookmark/folder operations are not all equal, and clients need truthful hints.

### 5. Untrusted-content framing

AutoCraft consistently treats issue/PR text as untrusted and tells the agent so in instructions (`src/dash/mcp/index.ts:59-67`). ArtCraft prompts, filenames, comments, model descriptions, TTS text, and generation outputs deserve the same treatment. Do not let upstream media metadata become instructions to the agent.

### 6. Single write gate and auditability

Only `src/github/write.ts` can write to GitHub, with compiler-enforced guards and audit rows (`src/github/write.ts:1-6`). ArtCraft MCP should similarly centralize upstream mutations in a small audited request layer rather than scattering writes across tools.

### 7. Scoped internal tools rather than general access

The `items` MCP server exposes exactly four read-only snapshot tools instead of broad database/GitHub access (`docs/pipeline.md:537`). ArtCraft MCP tools should remain capability-shaped and bounded rather than exposing a generic “run arbitrary API request” escape hatch.

### 8. Honest automation semantics

AutoCraft distinguishes actions performed automatically, actions requiring approval, unsupported cancellation, and uncertain outcomes. ArtCraft MCP should not claim successful cancellation/deletion/generation unless upstream confirms it.

## Interoperability opportunities with artcraft-mcp

There is little direct runtime interoperability today because AutoCraft does not use the ArtCraft media API. Opportunities are architectural rather than functional:

1. **MCP-to-MCP composition.** A maintainer agent could use AutoCraft's MCP Connector for repository workflow and artcraft-mcp for media generation in the same client session. This requires no formal integration, but consistent auth errors, tool naming, and result formatting would improve the combined experience.
2. **Shared conventions.** Adopt similar token prefixes, expiry behavior, tool annotations, instructions, role-aware listing, and strict validation so users experience both official MCP servers consistently.
3. **Issue/PR evidence loop.** AutoCraft could theoretically file or track upstream issues while artcraft-mcp supplies reproduction evidence generated through ArtCraft. That is an external workflow, not a current integration point.
4. **Gateway coexistence.** `craft-ai-integrations` has its own Craft-app MCP gateway; AutoCraft exposes a dashboard MCP server; artcraft-mcp exposes ArtCraft platform tools. They should remain separate services because their trust domains differ: local app automation, repository maintenance, and cloud media/account mutation.
5. **User-local task model.** If artcraft-mcp ever supports long-running local render/download workers, AutoCraft's lease/heartbeat/scope separation is the best upstream reference inspected.

## What not to copy blindly

- AutoCraft's MCP server implements a simplified Streamable HTTP subset (one JSON-RPC POST/response, no streams) and older protocol versions. artcraft-mcp is already built around a newer Rust MCP SDK and should not regress protocol conformance to copy AutoCraft.
- Its pipeline/database/queue complexity is far beyond what artcraft-mcp needs for upstream inference-job polling.
- Its TypeScript implementation and Node-specific sandboxing do not map directly onto the Rust MCP server.
- Its GitHub-specific trust and approval vocabulary should not be imported wholesale into media-generation tools.

## Bottom line

AutoCraft is a sophisticated repository-maintenance agent platform. It is not a client or automation layer for the ArtCraft media API. Its value to this rebuild is as a mature reference for secure MCP tool exposure, personal credential lifecycle, scoped automation, untrusted-content handling, and job execution boundaries. Borrow the design discipline, not the domain model.
