# Y Code (`ycode`)

Start with the [user guide and complete command reference](USER_GUIDE.md) for copy-and-paste setup, daily workflows, interactive controls, skills and Context7/MCP.

Y Code is a Rust terminal coding assistant with configurable LLM providers. Run `ycode` in a project to chat, inspect files, search source, propose exact edits, and execute approved commands. OpenRouter remains the default; custom OpenAI-compatible endpoints, Anthropic Messages and Gemini generateContent are supported. No background daemon is required.

## Multi-model and multi-provider teams

### Configure once for every project

```powershell
ycode config init
ycode config path
```

Edit the generated file at `%USERPROFILE%\.config\ycode\config.json` on Windows, or `~/.config/ycode/config.json` on Unix. Every new `ycode` invocation reads it automatically. `config init` creates a template with provider definitions and model placeholders; replace `YOUR_MANAGER_MODEL` and `YOUR_WORKER_MODEL` with your model IDs. It never overwrites an existing file. For example:

```json
{
  "defaults": {
    "provider": "openrouter",
    "model": "YOUR_MANAGER_MODEL",
    "worker_model": "YOUR_WORKER_MODEL",
    "multi_model": true,
    "plan": true,
    "sudo": true,
    "team": true,
    "workers": 3,
    "team_budget": 120,
    "max_steps": 30
  },
  "providers": {
    "openrouter": {
      "url": "https://openrouter.ai/api/v1/chat/completions",
      "format": "openai",
      "key_env": "OPENROUTER_API_KEY"
    }
  }
}
```

Then run `ycode` from any project without repeating provider/model flags. Keys remain in your environment; configure them once in your user environment to reuse them across terminals. The global file stores environment-variable references, not plaintext keys, and is never copied into project design context or worker prompts. Approval flags remain invocation-specific.

`defaults` also accepts `worker_provider`, `developer_model`, `developer_provider`, `reviewer_model`, `reviewer_provider`, `qa_model` and `qa_provider`. Per-role settings take priority over common worker defaults. `workers` accepts 1–8, `team_budget` 1–1000 and `max_steps` 1–100.

For project overrides, use `ycode.config.json` with the same `defaults`/`providers` schema and only the values you need to change. Omitted or null default fields retain global values. Existing project `ycode.providers.json` still works and overrides provider definitions with the same alias. Precedence for new sessions is **CLI options > project settings > global settings > built-in/environment defaults**. Resumed sessions preserve their saved models, providers and modes; explicit CLI flags still override them. Limits not stored in sessions, such as `max_steps`, use current configuration. Provider definitions always load from current files.

Use `YCODE_CONFIG` or `ycode --config PATH` to select another global file (`--config` wins), or `--no-global-config` to ignore it while retaining project settings. A missing default global file is optional; an explicitly selected missing file, malformed configuration or unsupported field fails with a diagnostic. `ycode doctor` checks the resolved provider/model and credentials without a live API call.

The manager uses your selected model for planning, design context and coordination. Developer, reviewer and QA workers can each use another model and API provider. Multi-model routing is enabled by default; unconfigured roles inherit the manager. Press **Tab** to switch configured worker routing on or off immediately, including while work is running. Off uses the manager's provider and model for new workers; on restores your worker settings. Running workers finish with the client they started with. Tab preserves unsent text and does not approve a tool action. Settings survive session resume.

```powershell
ycode --model YOUR_MANAGER_MODEL --worker-model YOUR_SMALL_MODEL
# Configure individual roles, or change them interactively:
# /models developer YOUR_DEVELOPER_MODEL
# /models reviewer YOUR_REVIEW_MODEL
# /models qa YOUR_QA_MODEL
# /multimodel off
# /multimodel on
```

For APIs beyond OpenRouter, create `ycode.providers.json` in the project. Use full endpoint URLs and environment-variable references; inline keys are rejected. An example is provided in `ycode.providers.example.json`. Provider names are your own aliases, independent of model IDs.

```json
{
  "manager_api": {
    "url": "https://YOUR_HOST/v1/chat/completions",
    "format": "openai",
    "key_env": "MANAGER_API_KEY"
  },
  "local_worker": {
    "url": "http://127.0.0.1:11434/v1/chat/completions",
    "format": "openai",
    "key_env": null
  }
}
```

```powershell
ycode --provider manager_api --model YOUR_MANAGER_MODEL --worker-provider local_worker --worker-model YOUR_LOCAL_MODEL
ycode --provider manager_api --model YOUR_MANAGER_MODEL doctor
# During an interactive session, configure workers with:
# /models worker YOUR_LOCAL_MODEL local_worker
# /models reviewer YOUR_REVIEW_MODEL ANOTHER_PROVIDER_NAME
# /models worker inherit
```

Specific `--developer-provider`, `--reviewer-provider` and `--qa-provider` flags override `--worker-provider`. `--no-multi-model` retains worker configuration while routing every role through the manager. `/model MODEL` changes the manager model while retaining project context; `--provider` selects its API when starting/resuming. Provider configuration is read at startup. Keys are resolved only for the active manager or dispatched workers; unused providers require no credentials. Environment variables take precedence over project `.env`, which loads `*_API_KEY` variables and the existing OpenRouter settings. Custom credential variable names also work when set in the process environment.

The `openai` format uses Chat Completions with function tools and bearer authentication; it supports compatible hosted and local servers. The `anthropic` format uses Messages with `x-api-key` and `anthropic-version`; the `gemini` format uses generateContent with `x-goog-api-key` and a `{model}` URL placeholder. Gemini reasoning signatures are preserved across tool rounds. These native adapters follow the [Anthropic tool lifecycle](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls) and [Gemini function-calling protocol](https://ai.google.dev/gemini-api/docs/function-calling/).

Use a model that supports function tools and the configured endpoint's request schema. APIs with other formats or authentication schemes need a compatible gateway or an additional adapter; this is not a claim of compatibility with every proprietary API. Plain chat streaming currently supports only `openai`; omit `--stream` for Anthropic/Gemini. URLs require HTTPS or loopback HTTP and cannot contain embedded credentials or query strings. Native provider defaults are 300 seconds and 4096 output tokens. Live credentials, provider/model availability and model quality have not been verified by the offline tests.

## Install

Rust 1.89 or newer and Cargo are required to build from source.

```powershell
cargo install --path crates/y-cli --bin ycode --locked --force
ycode --help
```

Cargo installs `ycode.exe` in `%USERPROFILE%\.cargo\bin` on Windows, or `~/.cargo/bin` on Unix. Add that directory to PATH if needed. The installer does not modify your shell profile. For a project-local build:

```powershell
cargo build --release -p y-cli --bin ycode --locked
.\target\release\ycode.exe --help
```

## Morning OpenRouter test

In PowerShell, set your key for the current terminal and run:

```powershell
$env:OPENROUTER_API_KEY = "YOUR_OPENROUTER_KEY"
ycode doctor
ycode ai --stream "Say hello in one sentence."
ycode -p --read-only "Read README.md and explain how this project works."
ycode
```

Alternatively copy `.env.example` to `.env` in your project and replace the key placeholder. Both the CLI and optional daemon load that file. Existing environment variables take precedence; `--model` overrides the configured model. The default model is `openai/gpt-4o-mini`; select another OpenRouter tool-capable model with `--model` or `OPENROUTER_MODEL`.

The live smoke-test script rebuilds the CLI and checks plain and streamed chat; optionally it tests source inspection:

```powershell
.\scripts\test_openrouter.ps1 -Agent
```

Requests use your OpenRouter credits. There is no silent mock fallback when the key is missing or the provider fails. No live provider call is included in the offline tests. The protocol follows [OpenRouter's tool-calling documentation](https://openrouter.ai/docs/guides/features/tool-calling).

## Daily use

Interactive `ycode` opens a full-screen terminal workspace with the composer anchored at the bottom and separate manager/developer/reviewer/QA model settings in the header. Type `/` or press Ctrl+P for a command picker, including manager and worker model configuration entries; arrows select, Enter inserts, and Esc closes. Use `/model MODEL` for the manager and `/models worker MODEL [PROVIDER]` for workers. Tab retains its multi-model toggle. PageUp/PageDown browse the bounded UI transcript, resizing reflows the display, and exit restores the shell screen. Concurrent output preserves unsent input and history position. Noninteractive commands retain plain output. `NO_COLOR` disables colors; unsupported terminals use line input. See [USER_GUIDE.md](USER_GUIDE.md#terminal-interface).

```powershell
ycode "Find the cause of this project's failing tests"
ycode --cwd C:\projects\my-app
ycode --model YOUR_PROVIDER/MODEL
ycode --resume
ycode --resume=SESSION_ID
ycode sessions
ycode -p --read-only "Review error handling in this repository"
"Explain this change" | ycode -p
```

Interactive commands: `/help`, `/session`, `/clear`, `/exit`. A prompt supplied on the command line starts the conversation. `-p` runs one turn and exits. In agent mode, each turn may use up to 30 model calls and 100 tool invocations; set `--max-steps` between 1 and 100 to change the model-call limit. `ycode ai` is plain chat without repository tools; `--stream` streams that mode. Agent responses arrive after each model call rather than token by token.

Tools: `list_files`, `read_file`, `search` (literal text), `write_file` (create only), `edit_file` (one exact unique replacement), and `run_command`. New files require an existing parent directory. File content is UTF-8, up to 1 MiB; tool results are capped at 32 KiB. Reads support line ranges. Listing skips secrets, links, and common build directories; traversal stops at depth 12 or 5,000 entries. Search inspects at most 2,000 files per call and returns at most 100 matches. Nested repository instructions can be read with the tools; root `AGENTS.md` is loaded automatically.

File changes show the proposed replacement or new content and require approval. Commands show the exact script and require approval. Empty approval input denies the action. `--read-only` excludes edit and command tools. Non-terminal input denies actions unless the invocation explicitly includes `--allow-write` or `--allow-shell`; those options preapprove their respective action categories for that invocation. They can be combined. Read-only mode conflicts with either option.

Shell commands run with normal OS filesystem and network access, from the selected workspace. Approval is a permission gate, **not an OS security sandbox**. The inherited environment is reduced to build-tool/system paths; API keys and inherited service credentials are removed. Tools can still access credentials through home directories or other files when an approved command does so. Commands have a 120-second timeout and bounded stdout/stderr (8 MiB per stream). Credentials are redacted from captured stderr before it is returned or logged, and argv is never logged. Windows uses a Job Object for cleanup; Unix uses a process group, which is only signalled while the child is still running — a reaped child's pid may already have been recycled. These are lifecycle controls, not protection from hostile commands escaping groups/jobs or racing startup. Treat repositories and model-generated commands as untrusted and inspect approvals.

Sessions are written atomically to `.ycode/sessions/`, with Unix file mode 0600 and inherited user-directory permissions on Windows. They contain conversation and tool output; keep them private. `.ycode/` and `.env` are gitignored. One agent invocation owns the workspace lock at a time. Ctrl+C exits with code 130; use `--resume` to continue. Interrupted tool calls are closed with an interruption result and never replayed automatically; inspect for partial side effects. Conversation context is capped at 256 KiB; `/clear` starts a new session when the limit is reached. There is no automatic conversation compaction; saved design context is an explicit optional mode.

Provider connections are pooled, have a connect timeout and a bounded overall deadline, and disable redirects. Streaming uses a per-read idle timeout rather than a whole-request deadline, so a slow-but-progressing generation is not truncated; partial output is returned alongside the error instead of being discarded. HTTP bodies, SSE lines (256 KiB), SSE events (1 MiB), and tool output are bounded inside the parsers themselves, not just at their call sites. Explicit 429 and 503 rejections retry at most twice, honoring numeric Retry-After up to 30 seconds; ambiguous network failures are not retried. Exit codes: 0 success, 1 runtime/configuration/provider error, 2 invalid command line, 130 interrupted.

## Native MCP servers

Configure MCP once in your global `config.json` using a `mcp_servers` object, or use the common `mcpServers` object in project `.mcp.json` / `ycode.mcp.json`. Project `ycode.config.json` accepts either spelling. Project settings override matching global aliases, `.mcp.json` overrides project settings, and `ycode.mcp.json` has final priority. Configuration does not start servers until an approved connection is needed.

```json
{
  "mcp_servers": {
    "git": {
      "command": "uvx",
      "args": ["mcp-server-git", "--repository", "${YCODE_WORKSPACE}"],
      "read_only_tools": ["git_status", "git_diff", "git_log", "git_show"]
    },
    "remote": {
      "type": "http",
      "url": "https://your-server.example/mcp",
      "bearer_env": "MY_MCP_TOKEN"
    }
  }
}
```

Supported transports are `stdio` (default for `command`), Streamable HTTP (`http`, default for `url`) and deprecated HTTP+SSE (`sse`). URLs require HTTPS or loopback HTTP and cannot embed credentials/query/fragment. Child servers run with a reduced environment. `env_env` maps child variable names to existing environment variable names; `headers_env` does the same for HTTP headers. `env` also accepts literal non-secret settings and `${VARIABLE}` references. Prefer environment references for credentials. `${YCODE_WORKSPACE}` expands to the current project or worker checkout. Plaintext MCP configuration is excluded from source-reading tools, design snapshots and worker Git snapshots. `disabled: true` disables an alias; `timeout_secs` accepts 1–120 (default 60).

```powershell
ycode mcp
ycode mcp check git
ycode mcp tools git
ycode --allow-mcp mcp check git
ycode --allow-mcp mcp call git git_status
ycode --no-mcp
```

These MCP commands do not require an LLM key. `check` verifies the negotiated protocol and all advertised tools/resources/templates/prompts catalogs. `call SERVER TOOL --arguments JSON`, `read SERVER URI`, and `prompt SERVER NAME --arguments JSON` support direct approved requests; `isError: true` from a tool makes a direct CLI call exit nonzero. Interactive `/mcp` lists configured aliases. In agent requests the model uses `mcp_list` to discover schemas on demand, then `mcp_call`, `mcp_read_resource` or `mcp_get_prompt`. The manager and workers use the same configuration, with local servers started in each worker's own checkout.

MCP connections and external actions require separate approval; `--allow-write` / `--allow-shell` do not grant MCP permission. `--allow-mcp` preapproves MCP for this invocation only. Plan/read-only/reviewer/manager restrictions still apply: tool calls require the exact `read_only_tools` allowlist, resource reads require `allow_read_resources: true`, and prompts require `allow_prompts: true`. Server annotations never grant permission. Catalog inspection is permitted after approving the connection. Starting a server runs code with normal OS access; approval and roots do not create an OS sandbox.

Windows `.cmd` launchers such as `npx` may require an explicit `cmd.exe` command with `/d`, `/c` arguments. A native executable (`uvx.exe`) or `node` plus an installed server's absolute script path avoids launcher ambiguity. MCP servers needing browser OAuth, sampling, elicitation, app UI or task orchestration have explicit limits; tokens/custom headers and core tools/resources/prompts are supported. See [MCP_COMPATIBILITY.md](MCP_COMPATIBILITY.md) for the tested versions, real-server checks, limits and reproduction commands. A starting configuration is available in `ycode.mcp.example.json`.

### Context7

Context7's hosted MCP server works directly, including library resolution and documentation queries in plan mode. Merge the `mcp_servers.context7` entry from [ycode.context7.example.json](ycode.context7.example.json) into `%USERPROFILE%\.config\ycode\config.json` to reuse it across projects, or copy the example to a project's `.mcp.json`. Preserve your other settings when merging.

```powershell
ycode --allow-mcp mcp check context7
ycode --allow-mcp
```

Then ask, for example: “Use Context7 to check the current React documentation before planning this feature.” The configured read allowlist lets managers, reviewers and plan-mode requests use both documentation tools.

The live Windows check succeeded without a Context7 key. Anonymous access and quotas may change. For authenticated access, add `"bearer_env": "CONTEXT7_API_KEY"` inside the server entry and set that environment variable before launching ycode. This uses the official [Context7 bearer-header setup](https://context7.com/docs/resources/all-clients); browser OAuth remains unsupported. The OpenRouter/provider key is separate.

Repeat the optional live check with `node scripts/verify_context7.mjs` after building, or pass the installed executable path. It uses a temporary project, tests discovery and both tools in plan mode, and writes metadata-only results to `target/context7-interoperability/results.json`. It uses an existing Context7 environment key when available. These service-dependent checks are separate from offline tests and CI's local reference-server checks.

## Portable skills from Codex, Claude Code and Antigravity

Existing `SKILL.md` packages work directly, without conversion or copying. Discovery is enabled by default. Inspect the catalog and compatibility diagnostics without an API key:

```powershell
ycode skills
ycode skills show NAME
ycode --skill-dir C:\my-skills skills
ycode --skill NAME "Plan a change using this skill"
```

In a conversation, use `/skill NAME task arguments`, `/NAME task arguments`, or mention `$NAME` in your request. `/skills` lists packages; `/skill clear` removes explicit selections. Up to four explicit selections persist across turns and resume. Their current instructions are shared with the project manager and workers, including in saved design mode. Other skills expose only name/description metadata until the model calls `load_skill`; references and scripts are read on demand with `read_skill_resource`.

Project discovery checks `.ycode/skills`, `.agents/skills`, `.claude/skills`, `.codex/skills`, `.agent/skills` and `.gemini/skills`, starting at the working directory and stopping at the Git root. Outside Git, only the working directory is checked. User discovery checks `~/.config/ycode/skills`, `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, `~/.gemini/config/skills`, `~/.gemini/antigravity-cli/skills`, `~/.gemini/antigravity/skills`, `~/.gemini/skills`, and `$CODEX_HOME/skills` when configured. On Windows, `~` means `%USERPROFILE%`. These cover current and legacy locations described in the official [Codex](https://learn.chatgpt.com/docs/build-skills), [Claude Code](https://code.claude.com/docs/en/skills), and [Antigravity](https://antigravity.google/docs/skills?app=antigravity-ide) documentation.

For other collections or plugin packages, add `--skill-dir DIRECTORY`, or merge this section into your global configuration:

```json
{
  "skills": {
    "enabled": true,
    "discover_external": true,
    "paths": ["C:/my-shared-skills"]
  }
}
```

Paths can point to a package containing `SKILL.md` or a collection of packages. Relative configured paths resolve against the configuration file; use absolute paths for shared locations. Project settings prepend their paths to global paths. Name lookup prefers the nearest project package, then command-line directories, configured directories, and user locations. Duplicate names remain separate; use the exact `id` from `ycode skills` to select another copy. Set `discover_external: false` to retain only native/configured locations, or use `--no-skills` to disable discovery for an invocation.

Markdown instructions, bounded YAML metadata, relative references, `${CLAUDE_SKILL_DIR}`, `$ARGUMENTS`, `$ARGUMENTS[N]` and `$N` are supported. Positional arguments use whitespace splitting. `disable-model-invocation: true` requires explicit selection; `user-invocable: false` permits model selection only. A missing description makes a package manual-only. Scripts use the ordinary `run_command` approval and execute from the project directory; models must use the package's script path. Plan mode continues to prohibit shell commands and edits.

Host-specific `context: fork`, agent/model overrides, hooks, `allowed-tools`, embedded MCP declarations and dynamic shell injection produce diagnostics and are not activated. Configure MCP dependencies through ycode settings and use the native `mcp_list`/`mcp_call` bridge; a skill's declarations do not launch servers or grant permissions. App runtimes and orchestrator `skill://` resources remain unsupported. Portable instructions that depend on those features may need adaptation. Linked packages/resources, traversal, secret resources, invalid YAML and oversized files are rejected. Discovery is bounded to 128 skills, 2,000 visited paths and four nested directory levels; manifests and individual resources are limited to 32 KiB. No external skill installer or plugin runtime is included in this stage.

## Plan and saved design modes

New `ycode` sessions default to **plan mode with saved design context enabled**. Startup waits for your input without scanning source or making a model request. Your task produces a read-only plan; after successful source inspection, the harness creates or updates the design using that turn's observations. Run `/implement` to continue with that plan, or `/implement additional direction` to refine the implementation request. This turns plan mode off and uses the existing edit/command approvals. Use `/plan on` before planning another task, or `/plan off` for direct work. To opt out at startup, use `--no-plan` and/or `--no-sudo`. Resumed sessions retain their saved settings.

```powershell
ycode --plan --sudo "Plan a fix for the parser"
ycode --plan "Design a caching layer"
ycode --sudo "Investigate this issue using the saved architecture"
ycode --resume --no-plan --sudo "Implement the saved plan"
```

Plan mode permits inspection but blocks file edits and shell commands, including when `--allow-write` or `--allow-shell` is set. Each successful planning turn saves its proposal to `.ycode/plans/SESSION_ID.md`. The latest proposal replaces that session's plan file; earlier plans remain in the session transcript. `/plan` toggles the mode; `/plan on` and `/plan off` set it explicitly.

Sudo mode means **pseudocode/design context**, with no elevated OS privileges. `--sudo` (alias `--design-mode`) enables a concise architecture/pseudocode summary built from prompted source inspection. Greetings and mode toggles alone do not generate it. The summary is exported to `.ycode/design.md`; `.ycode/design-context.json` is the canonical cached summary and file metadata. Use `/sudo refresh` for an explicit read-only architecture scan. Team implementation without a saved design builds a task-scoped reference after you submit an implementation request. Editing the Markdown export does not change the canonical context.

With sudo mode enabled, subsequent requests use the saved design, the session's latest plan (up to 32 KiB), up to six abbreviated prior user requests, and the current turn's messages and targeted tool results. Old assistant and source/tool payloads stay in the local session instead of being resent. The model can still read relevant source and must verify source before editing; a summary cannot resolve every issue alone. File size/mtime checks warn when the design may be stale; this check is approximate and does not hash contents. The scan uses the existing listing limits and exclusions.

Interactive controls: `/sudo` toggles the mode; `/sudo on`, `/sudo off`, `/sudo status`, and `/sudo refresh` are explicit controls. `/design` is an alias. The prompt displays active modes. Both settings persist across `--resume` and `/clear`; `--no-plan` and `--no-sudo` disable them explicitly. All artifacts are private local state under gitignored `.ycode/`, subject to the same file/link checks as session storage.

After a completed turn, sudo mode updates the main design when source was inspected or files changed. Refresh carries forward the previous architecture, reuses the current turn's targeted reads and uses reported outcomes as untrusted evidence. Newly inspected areas can expand the summary even when source is unchanged. Proposed plans remain separate from observed implementation. Turns without inspection or source changes do not trigger generation. If refresh fails, completed work and the previous design remain available; the CLI reports the failure and `/sudo refresh` retries it. Refresh requests consume provider credits.

Use `/model` to show the selected model, or `/model PROVIDER/MODEL` to switch models between turns without discarding the session, design, or plan. The selection persists on resume; `--model` explicitly overrides it. The new model must support tool calling and be available through your OpenRouter account; availability is checked by the next provider request, not by the local switch command.

## Project manager and worker agents

New sessions enable team implementation by default. Planning stays read-only; `/implement` starts a project manager that assigns small developer tasks and dependent reviewer/QA tasks. The manager may assign multiple agents of each role. Workers exchange design/feature comments through the live coordination board and return structured reports to the manager. A readiness comment alone never releases a dependency: the harness requires an accepted developer report and successful integration first.

Team mode requires **Git, a repository-root workspace, and an initial commit**. Developer and verification agents use detached worktrees based on a temporary snapshot of the current allowed files, including uncommitted source. The user's branch, HEAD and index are preserved. Known private files/state are excluded even when tracked. Developer file tools are restricted to declared owned files; out-of-scope changes are rejected before integration. Conflicting patches fail and their checkouts remain available. Repository links remain excluded. Git hooks are disabled for harness-managed Git operations. Worktrees provide checkout isolation, not an OS filesystem/network sandbox; approved developer and QA shell commands still have normal OS access.

Reviewers have inspection tools without file edits or shells. QA has inspection and approved command tools, but no file-edit tools. Reviewers must record actual source inspection, and passing QA requires observed successful command evidence. Source changes made by verification agents invalidate their reports. Review and QA run against the integrated revision after developer tasks settle. Every accepted developer task needs independent review and QA coverage on the current revision and current user direction. The manager can publish its final summary only when those gates pass. Command success and model review do not prove test coverage or feature correctness; the manager must assess the evidence. Failed tasks, budgets and provider errors produce an incomplete/error result, never a fabricated success.

```powershell
ycode                         # plan + design + team defaults
ycode --workers 3 --team-budget 120
ycode --developer-model PROVIDER/MODEL --reviewer-model PROVIDER/MODEL --qa-model PROVIDER/MODEL
ycode --no-team                # retain the single-agent implementation workflow
```

All roles initially use the selected model; role flags or per-task manager assignments can override it. Defaults are three concurrent workers (configurable 1–8), 24 tasks, 120 total manager/worker model calls per run (configurable 1–1,000), and the existing per-agent `--max-steps` and tool limits. Dependencies and worker slots bound scheduling. Board views abbreviate long reports/comments; the manager can request full assignments/reports and paged command records. Full board state and transcripts remain local. Role settings and concurrency persist on resume.

During a team run, type additional information to steer the manager and workers. New direction invalidates earlier verification; an in-flight developer result from the earlier direction is held for reconciliation rather than integrated. `/model PROVIDER/MODEL` changes future manager rounds and inherited models for newly started workers; active workers and explicit role overrides keep their selected model. `/team status` prints progress, and `/stop` cancels the run while preserving reports/checkouts. File and command approvals use `/approve y` or `/approve n`; approval input is routed separately from steering and serialized across workers. The action preview names its checkout. Empty approval input denies the action.

Outside a run, use `/team on`, `/team off`, `/team status`, or `/team resume`. Resume marks interrupted work for inspection and reconciliation; it never automatically replays interrupted edits/commands. Explicit resume renews the total call budget and requires fresh verification. Team state lives in `.ycode/teams/RUN_ID/board.json`, `coordination.md`, per-task transcripts, and retained worker worktrees. The coordination document embeds the shared design with task reports and comments. These artifacts are gitignored and contain private project data. Checkouts are retained for inspection/recovery and can consume substantial disk space; there is no automatic worktree cleanup yet.

After accepted completion, the existing sudo workflow refreshes the main design from observed integrated source. Worker agents cannot recursively spawn agents; the project manager owns all assignments. The CLI still holds one workspace lock for the coordinated run. The experimental `y-mesh` queue is separate from this provider-backed coordinator.

## Verification

These are the exact commands CI runs:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo run --locked -p y-cli --bin ycode -- --help
cargo build --release --locked -p y-cli --bin ycode
```

Performance guardrails are measured from a **release** build, because the thresholds are budgets and a debug build does not measure what ships. `y-bench` exits non-zero if any guardrail is breached:

```powershell
cargo build --release --locked -p y-bench
.\target\release\y-bench.exe
```

`scripts/run_all_benchmarks.sh` (or `.ps1`) runs format, clippy, tests, the release build, and the guardrails in sequence, and stops on the first failure.

Offline tests use loopback mock servers for the entire model → tool → model sequence, conversation resume, provider error handling, retries, and streaming. They also check permissions, path boundaries, atomic edits, workspace locks, process timeout/exit codes, configuration precedence, SQLite durability, arena alignment and scrubbing, prompt-injection defanging, SSE framing, IPC message boundaries, and the fuel and output caps. CI additionally runs the declared MSRV (1.89), a `cargo deny` advisory/license audit, and the guardrails. Release binaries are built for an explicit `x86-64-v2`/`apple-a14` baseline rather than `target-cpu=native`, so artifacts stay portable across hosts.

## Architecture and current scope

The production-facing command is `ycode` in `crates/y-cli`, with the provider transport in `crates/y-gateway` and the native MCP client using the official Rust SDK. Other crates are retained for systems research and testing. The original `SessionReactor` executes a **local pipeline with no gateway call bound** — its completion text is a fixed local string, and `harness_overhead_ns` measures the whole turn because there is no inference interval to subtract yet. The Wasm, mesh, and original `y-mcp` sanitizer scaffolds are not integrated into the CLI; `y-inference-worker` is a stub that exits non-zero rather than advertising a handshake it cannot perform.

The optional `y-daemon` serves a small dashboard on `http://127.0.0.1:4040/`. It rejects cross-origin browser requests, ambiguous/oversized HTTP messages, and missing provider configuration, and reports no fabricated metrics.

**`POST /api/turn` is disabled unless `Y_DAEMON_TOKEN` is set.** When set, the request must carry `Authorization: Bearer $Y_DAEMON_TOKEN`; without it the endpoint returns 503 and no model credits are spent. Loopback binding is not access control — any local process can reach the port.

Daemon state lives in a per-user directory (`%LOCALAPPDATA%/y-code` on Windows, `$XDG_STATE_HOME/y-code` or `~/.local/state/y-code` elsewhere), not the current directory. Its Unix control socket is created with mode `0600`, and IPC frames are length-prefixed and bounded at 1 MiB.

The historical architecture reports and `PROJECT.md` describe research goals, not shipping guarantees. Claims about zero allocations, shared-memory IPC, enforced memory limits, measured resource SLAs, or full sandbox isolation should not be inferred from those reports. See `PRODUCTION_READINESS.md` for the implementation assessment and remaining release limitations. This is a tested local coding-assistant baseline, not full Claude Code feature parity: no IDE bridge, automatic compaction, general OS sandbox, or managed distribution/update service; native MCP compatibility has the limits documented above.
