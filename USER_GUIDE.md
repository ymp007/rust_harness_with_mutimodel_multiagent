# ycode user guide

Verified against the installed CLI on October 10, 2026. This guide covers the user-facing commands, startup options and interactive controls currently implemented.

**Code blocks marked `powershell` run in your terminal. Blocks marked `text` run inside an interactive `ycode` session.** Replace uppercase placeholders such as `YOUR_MODEL_ID`, `SKILL_NAME` and `C:\projects\my-app` before running them. A model ID comes from your API provider; a provider name is an alias in your configuration. Use models that support function tools.

## Contents

- [First run](#first-run)
- [Configure once for every project](#configure-once-for-every-project)
- [Terminal interface](#terminal-interface)
- [Daily commands and sessions](#daily-commands-and-sessions)
- [Plan, saved design and implementation](#plan-saved-design-and-implementation)
- [Project manager and worker agents](#project-manager-and-worker-agents)
- [Models and API providers](#models-and-api-providers)
- [Portable skills](#portable-skills)
- [MCP and Context7](#mcp-and-context7)
- [Approvals and cancellation](#approvals-and-cancellation)
- [Complete interactive command reference](#complete-interactive-command-reference)
- [Complete terminal option reference](#complete-terminal-option-reference)
- [Files, troubleshooting and verification](#files-troubleshooting-and-verification)

## First run

If ycode is already installed, check it:

```powershell
ycode --version
ycode --help
```

To install from source, run this from the harness repository with Rust 1.89+ and Cargo installed:

```powershell
cargo install --path crates/y-cli --bin ycode --locked --force
```

If PowerShell cannot find `ycode`, add Cargo's executable directory to the current terminal's PATH:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
ycode --help
```

Enter your OpenRouter key without putting it in terminal history. This sets it for the current terminal and its child processes:

```powershell
$taskKey = Read-Host "OpenRouter API key" -AsSecureString
$taskKeyPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($taskKey)
try {
    $env:OPENROUTER_API_KEY = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($taskKeyPointer)
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($taskKeyPointer)
    $taskKey.Dispose()
    Remove-Variable taskKey, taskKeyPointer
}
```

Check configuration locally, then make a small live request:

```powershell
ycode doctor
ycode ai --stream "Say hello in one sentence."
```

`doctor` makes no API call. Chat, planning, design generation and agent work use your provider's credits. OpenRouter is the default provider; the built-in model is `openai/gpt-4o-mini` unless overridden. Provider availability is checked by live requests.

Start in your project:

```powershell
Set-Location "C:\projects\my-app"
ycode
```

Inside ycode:

```text
Inspect this project and plan a small improvement. Explain the affected files and how you will test it.
```

After reviewing the saved plan:

```text
/implement
```

Team implementation requires a Git repository root with an initial commit. For a folder that is not ready for team mode, use `ycode --no-team` for the single-agent workflow.

## Configure once for every project

Create a global template and show its location:

```powershell
ycode config init
ycode config path
notepad "$env:USERPROFILE\.config\ycode\config.json"
```

`config init` never overwrites an existing file. On Windows the default path is `%USERPROFILE%\.config\ycode\config.json`; on Unix it is `~/.config/ycode/config.json`.

The generated template contains model placeholders. Replace them before live use. This is a minimal global configuration; merge it with existing settings rather than overwriting unrelated entries:

```json
{
  "defaults": {
    "provider": "openrouter",
    "model": "YOUR_MANAGER_MODEL",
    "worker_model": "YOUR_WORKER_MODEL",
    "plan": true,
    "sudo": true,
    "team": true,
    "multi_model": true,
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
  },
  "mcp_servers": {
    "context7": {
      "type": "http",
      "url": "https://mcp.context7.com/mcp",
      "read_only_tools": ["resolve-library-id", "query-docs"]
    }
  }
}
```

For different worker roles, `defaults` also accepts `worker_provider`, `developer_model`, `developer_provider`, `reviewer_model`, `reviewer_provider`, `qa_model` and `qa_provider`. Role-specific values override common worker values.

The JSON stores key environment-variable names, not actual keys. To reuse the OpenRouter key you entered above in future terminals, optionally save it in your Windows user environment:

```powershell
[Environment]::SetEnvironmentVariable('OPENROUTER_API_KEY', $env:OPENROUTER_API_KEY, 'User')
```

This persists the credential in your user environment. Newly started terminals receive it; existing terminals retain their current values. Alternatively, use a private project `.env` containing `OPENROUTER_API_KEY=YOUR_KEY`; environment values take precedence. Keep `.env` out of version control.

Select an alternative global configuration or ignore the global file:

```powershell
ycode --config "C:\configs\ycode.json"
$env:YCODE_CONFIG = "C:\configs\ycode.json"
ycode
Remove-Item Env:YCODE_CONFIG
ycode --no-global-config
```

`--no-global-config` still loads project settings. Project `ycode.config.json` uses the global schema and can contain only your overrides. For new sessions, CLI options override project settings, which override global settings. Resumed sessions retain saved models/providers/modes unless explicit CLI options change them. Provider definitions load from current files at startup. Approval grants are never stored in global settings.

## Terminal interface

Interactive `ycode` fills the terminal window using an alternate screen, with the input composer anchored at the bottom. The header shows separate manager, developer, reviewer and QA models/providers; the footer shows Plan/Build and design/team/multi-model status. Project paths omit the Windows extended-path prefix. Exit or Ctrl+C restores your shell screen. This fills the terminal's current window; use your terminal's own maximize/full-screen shortcut to enlarge the window itself.

Use **PageUp/PageDown** to browse earlier output. The live UI retains up to 128 KiB/4,000 transcript lines; saved conversations/reports remain in `.ycode/`. Incoming output preserves your history position until you submit another command/request. Terminal resizing reflows the display. Noninteractive commands such as `doctor`, `mcp`, `ai` and `-p` retain plain terminal output.

Type `/` to filter a command picker, or press **Ctrl+P** to open it. Use **Up/Down** to choose, **Enter** to insert the command, then **Enter** again to submit. A fully typed exact command submits directly. **Esc** closes the picker without clearing your input. **Tab** always toggles multi-model routing, including when the picker is open. Left/Right, Home/End, Backspace/Delete edit the prompt; long text uses a horizontal viewport.

The picker includes “Set manager” and “Set all workers” entries. Select an entry, append your actual model ID, and submit. Inside ycode:

```text
/model YOUR_MANAGER_MODEL
/models worker YOUR_SMALL_WORKER_MODEL
/multimodel on
```

You can also set `/models developer MODEL`, `/models reviewer MODEL`, and `/models qa MODEL`, optionally followed by a configured provider alias. The header updates immediately. These choices persist on session resume; set global `defaults.model` and `defaults.worker_model` for defaults across new sessions/projects. Tab off temporarily routes every worker through the manager; Tab on restores the configured role choices. Running workers keep the clients they started with.

Approvals and worker messages preserve your unsent input. Set `NO_COLOR` to disable colors; redirected/noninteractive commands retain plain output. Terminals without native key support use line input.

```powershell
$env:NO_COLOR = "1"
ycode
Remove-Item Env:NO_COLOR
```

## Daily commands and sessions

```powershell
# Start a conversation in the current project.
ycode

# Start with a request, or select another workspace.
ycode "Find the cause of the failing tests and plan a fix."
ycode --cwd "C:\projects\my-app"

# One read-only turn, then exit.
ycode -p --read-only "Read README.md and explain this project."
"Review error handling and suggest improvements." | ycode -p --read-only

# Inspect and resume saved sessions in this project.
ycode sessions
ycode --resume
ycode --resume=SESSION_ID

# Plain chat: no repository tools.
ycode ai "Explain dependency injection in two paragraphs."
ycode ai --stream "Say hello in one sentence."

# Help for a particular command.
ycode help
ycode help mcp
ycode mcp call --help
ycode skills show --help
```

`-p` runs one turn. A resumed session can have planning disabled, so add `--plan` or `--read-only` when you require inspection only. Plain-chat `--stream` currently works with the `openai` API format; omit it for Anthropic/Gemini.

Inside ycode, `/session` shows the current session, `/clear` starts a new conversation while preserving configured modes and model routing, and `/exit` leaves. `/clear` also clears explicit skill selections.

## Plan, saved design and implementation

New sessions default to plan mode, saved design context, team mode and multi-model routing. Workers without overrides inherit the manager's model/provider.

1. Start ycode and describe the task.
2. It inspects relevant files and produces a read-only plan, then creates/updates the saved design from those observations. Startup itself makes no model request.
3. Review the plan and add direction if needed.
4. Run `/implement` to execute the latest saved plan with approvals.
5. After a completed turn, source inspection or changed files trigger a design update when design mode is enabled.

Inside ycode:

```text
/plan on
Plan an API endpoint for exporting reports. Include validation and tests.
/implement Use the existing authentication middleware.
/plan on
```

Explicit startup choices:

```powershell
ycode --plan --sudo "Plan a parser fix."
ycode --no-plan "Implement the parser fix and test it."
ycode --no-team --no-plan "Make a small local change."
ycode --resume --no-plan --sudo "Implement the latest saved plan."
ycode --no-sudo
```

`--no-plan` enables implementation directly; it does not grant edit/command approvals. Plan mode blocks source edits and shell commands even if approval flags are supplied. `--read-only` remains restrictive even after `/implement`.

**“Sudo” means saved pseudocode/design context, with no OS privilege elevation.** It reduces repeated source context while allowing targeted source reads. The model still needs to inspect relevant code before editing.

Inside ycode:

```text
/sudo status
/sudo refresh
/sudo off
/sudo on
```

`/design` is an alias for `/sudo`, including its subcommands. Refresh consumes provider credits. `.ycode/design.md` is an exported summary; editing it does not update the canonical `.ycode/design-context.json`. Use `/sudo refresh` after architectural changes or a failed automatic refresh.

## Project manager and worker agents

The manager assigns developer tasks and dependent review/QA tasks, coordinates reports and responds to new user direction. Workers share design and feature comments through the coordination board. Accepted developer work must have independent review and successful QA command evidence on the current integrated revision before the manager reports completion.

Before using team implementation, check your project:

```powershell
git rev-parse --show-toplevel
git log -1 --oneline
```

Run ycode from that repository root, with at least one existing commit. For a new repository, initialize it and commit the files you intend to track before starting team implementation. The harness uses isolated worktrees and retains them for inspection; it preserves your branch, HEAD and index.

```powershell
ycode --workers 3 --team-budget 120
ycode --team --workers 2 --max-steps 30
ycode --no-team
```

During a team run, type new information normally to steer work:

```text
Also preserve backward compatibility with the existing response format.
/team status
/model YOUR_NEW_MANAGER_MODEL
/stop
```

`/stop` cancels the current team run while retaining its state. Outside the run:

```text
/team status
/team resume
/team off
/team on
```

`/team resume` inspects interrupted work and reconciles it; it does not automatically replay interrupted actions. It renews the team call budget and requires fresh verification. The project manager owns worker creation; workers cannot recursively spawn agents.

Limits: `--workers` 1–8 (default 3), `--team-budget` 1–1000 total manager/worker model calls per run (default 120), `--max-steps` 1–100 calls per agent turn (default 30). A run supports up to 24 tasks. Worktree isolation is not an OS sandbox.

## Models and API providers

Use one model for all roles:

```powershell
ycode --model YOUR_MODEL_ID --no-multi-model
```

Use a manager and smaller workers, or choose each role separately:

```powershell
ycode --model YOUR_MANAGER_MODEL --worker-model YOUR_SMALL_MODEL
ycode --model YOUR_MANAGER_MODEL --developer-model YOUR_DEVELOPER_MODEL --reviewer-model YOUR_REVIEW_MODEL --qa-model YOUR_QA_MODEL
```

Inside ycode:

```text
/model
/model YOUR_NEW_MANAGER_MODEL
/models
/models worker YOUR_WORKER_MODEL
/models developer YOUR_DEVELOPER_MODEL
/models reviewer YOUR_REVIEW_MODEL
/models qa YOUR_QA_MODEL
/multimodel status
/multimodel off
/multimodel on
/models worker inherit
```

Press **Tab** to toggle multi-model routing, including while team work runs. Off routes newly started workers through the manager's model/provider; on restores configured worker choices. Existing workers finish with the clients they started with. Tab preserves your input and never approves an action. `/model` preserves the conversation, plan and design; it changes the model within the manager's current provider.

Configure additional APIs in global `providers`, project `ycode.config.json`, or project `ycode.providers.json`. The standalone providers file contains the provider map directly:

```json
{
  "local": {
    "url": "http://127.0.0.1:11434/v1/chat/completions",
    "format": "openai",
    "key_env": null
  },
  "anthropic": {
    "url": "https://api.anthropic.com/v1/messages",
    "format": "anthropic",
    "key_env": "ANTHROPIC_API_KEY"
  },
  "gemini": {
    "url": "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent",
    "format": "gemini",
    "key_env": "GEMINI_API_KEY"
  }
}
```

The local endpoint must already be running with your chosen model available. Set each remote API's environment key before using it. The supported formats are OpenAI-compatible Chat Completions, Anthropic Messages and Gemini generateContent; other proprietary protocols require an adapter/gateway. Use full HTTPS endpoint URLs, or loopback HTTP for local APIs. Inline keys and credentials embedded in URLs are rejected.

```powershell
ycode --provider openrouter --model YOUR_MANAGER_MODEL --worker-provider local --worker-model YOUR_LOCAL_MODEL
ycode --provider anthropic --model YOUR_ANTHROPIC_MODEL doctor
ycode --provider gemini --model YOUR_GEMINI_MODEL
ycode --developer-provider local --reviewer-provider openrouter --qa-provider local --developer-model YOUR_LOCAL_MODEL --reviewer-model YOUR_REVIEW_MODEL --qa-model YOUR_LOCAL_MODEL
```

Interactive worker-provider selection:

```text
/models worker YOUR_LOCAL_MODEL local
/models reviewer YOUR_REVIEW_MODEL openrouter
/models qa inherit
```

Select the manager provider at startup with `--provider`; there is no interactive `/provider` command. Restart/resume after changing provider definitions. Unused providers do not need credentials. Model IDs and model quality depend on the provider; the harness does not supply or validate a universal model catalog.

## Portable skills

Existing portable `SKILL.md` packages from Codex, Claude Code and Antigravity can be discovered directly. No conversion is needed for portable instructions/references. Packages in conventional project/user skill directories are discovered automatically; add other locations explicitly:

```powershell
ycode skills
ycode skills show SKILL_NAME
ycode --skill-dir "C:\skills" skills
ycode --skill-dir "C:\skills\my-package" skills show SKILL_NAME
ycode --skill SKILL_NAME "Use this skill to plan the requested change."
ycode --skill FIRST_SKILL --skill SECOND_SKILL
ycode --no-skills
```

Inside ycode:

```text
/skills
/skill SKILL_NAME optional arguments for the skill
$SKILL_NAME Plan this feature using the skill.
/skill clear
```

The `$SKILL_NAME` form belongs inside ycode; PowerShell treats `$` as a variable prefix. If passing it as a terminal prompt, use single quotes, for example `ycode '$SKILL_NAME Plan this feature.'`.

Up to four explicit skills remain active across turns/resume and are shared with workers. Instructions/references load on demand. Skill scripts use normal command approval and cannot run in plan mode. Diagnostics identify unsupported host hooks, `context: fork`, embedded MCP declarations, app-specific tools and permission/model overrides. Skill metadata never grants permission or starts an MCP server; configure its MCP dependency separately.

## MCP and Context7

Configure a server once in global `mcp_servers` to reuse it everywhere, or add project `.mcp.json` / `ycode.mcp.json` with a `mcpServers` map. The full global example above includes Context7. [ycode.context7.example.json](ycode.context7.example.json) is a smaller merge/copy template.

Server overrides apply in this order: global settings → project `ycode.config.json` → `.mcp.json` → `ycode.mcp.json`. Matching aliases are replaced. Servers connect lazily after approval.

```powershell
# List configured aliases without connecting.
ycode mcp

# Verify Context7 and inspect its tool schemas; no LLM key needed.
ycode --allow-mcp mcp check context7
ycode --allow-mcp mcp tools context7

# Enable approved MCP access for a conversation, or disable it entirely.
ycode --allow-mcp
ycode --no-mcp
```

Inside ycode:

```text
/mcp
Use Context7 to check the current documentation for the library involved, then plan the change.
```

Context7 discovery, `resolve-library-id` and `query-docs` were live-tested with the installed client, including plan mode. Anonymous access worked during verification; service quotas and availability can change. For API-key access add `"bearer_env": "CONTEXT7_API_KEY"` to its entry and set that environment variable before starting ycode. This key is separate from your LLM-provider key; browser OAuth is not integrated.

Direct Context7 calls use JSON arguments. The following examples target **PowerShell 7.3+**, whose native argument handling preserves JSON quotes. For Windows PowerShell 5.1, prefer the conversation workflow or the Node verification script below.

```powershell
ycode --allow-mcp --plan mcp call context7 resolve-library-id --arguments '{"libraryName":"React","query":"How to use the useState hook?"}'
```

Choose a library ID from that response, then substitute it below:

```powershell
ycode --allow-mcp --plan mcp call context7 query-docs --arguments '{"libraryId":"LIBRARY_ID_FROM_PREVIOUS_RESPONSE","query":"How to increment a counter with useState?"}'
```

General MCP commands, for a server configured under `SERVER_NAME`:

```powershell
ycode --allow-mcp mcp check SERVER_NAME
ycode --allow-mcp mcp tools SERVER_NAME
ycode --allow-mcp mcp call SERVER_NAME TOOL_NAME
ycode --allow-mcp mcp call SERVER_NAME TOOL_NAME --arguments '{"argumentName":"value"}'
ycode --allow-mcp mcp read SERVER_NAME "RESOURCE_URI"
ycode --allow-mcp mcp prompt SERVER_NAME PROMPT_NAME
ycode --allow-mcp mcp prompt SERVER_NAME PROMPT_NAME --arguments '{"argumentName":"value"}'
```

Use the tool/prompt's actual schema; omit arguments only when none are required. `check` aggregates all advertised catalogs; `tools` returns the first tool page. A direct tool result with `isError: true` exits nonzero.

Supported server transports: stdio (`command`/`args`), Streamable HTTP (`type: "http"`/`url`) and legacy SSE (`type: "sse"`/`url`). Windows `npx.cmd` may need an explicit `cmd.exe /d /c` launcher; remote Context7 avoids that dependency. [ycode.mcp.example.json](ycode.mcp.example.json) contains additional configuration examples.

Server fields:

| Field | Purpose |
| --- | --- |
| `type` | `stdio`, `http` or `sse`; inferred from command/URL when omitted |
| `command`, `args` | Local executable and argument array |
| `url` | HTTPS endpoint or loopback HTTP; no embedded credentials/query/fragment |
| `env` | Explicit child settings; `${VARIABLE}` references supported |
| `env_env` | Child environment name → existing environment variable name |
| `headers_env` | HTTP header name → existing environment variable name |
| `bearer_env` | Existing environment variable containing a bearer token |
| `read_only_tools` | Exact tool names allowed in plan/manager/reviewer/read-only modes |
| `allow_read_resources`, `allow_prompts` | Permit those operations in restrictive modes |
| `disabled` | Disable this alias when true |
| `timeout_secs` | Startup/request deadline, 1–120 seconds; default 60 |

`${YCODE_WORKSPACE}` expands to the current project/worker checkout. Approval is required even for starting a local server. `--allow-mcp` does not bypass read allowlists; server annotations do not grant permissions. Browser OAuth, sampling, elicitation, MCP Apps UI and long-running tasks are not integrated. See [MCP_COMPATIBILITY.md](MCP_COMPATIBILITY.md) for tested coverage and limits.

## Approvals and cancellation

Interactive source edits and commands display an action preview and require approval. Empty input denies the action. During a team run, respond through:

```text
/approve y
/approve n
```

At an ordinary approval prompt outside a team run, follow its displayed response format. Inspect the file change or command and its checkout before accepting.

For a trusted, noninteractive single-agent task, explicitly grant only the required action categories:

```powershell
ycode -p --no-team --no-plan --allow-write "Apply the agreed text change."
ycode -p --no-team --no-plan --allow-shell "Run the existing test command and report the result."
ycode -p --no-team --no-plan --allow-write --allow-shell "Implement the agreed change and run its tests."
```

These grants last for that invocation. MCP requires its own `--allow-mcp`. Noninteractive actions without grants are denied. `--read-only` conflicts with `--allow-write`/`--allow-shell`; plan mode still blocks source writes/shells even when grants are supplied. Approved commands and local MCP servers have normal OS access.

Press **Ctrl+C** to cancel and exit; resume later with `ycode --resume`. During team work `/stop` cancels the run. Interrupted actions are not replayed automatically; inspect possible partial changes before continuing.

## Complete interactive command reference

Enter these inside ycode, not in PowerShell. A bare toggle command switches its current state.

| Command | Action |
| --- | --- |
| `/help` | Show interactive help |
| `/session` | Show current session/model/modes |
| `/clear` | Start a new conversation; preserve modes/routing, clear explicit skills |
| `/exit`, `/quit` | Exit |
| `/plan`, `/plan on`, `/plan off` | Toggle/set planning |
| `/implement [direction]` | Implement the latest saved plan with optional direction |
| `/sudo`, `/sudo on`, `/sudo off` | Toggle/set saved design context |
| `/sudo status`, `/sudo refresh` | Inspect/regenerate design context |
| `/design [on\|off\|status\|refresh]` | Alias for `/sudo` |
| `/model [MODEL_ID]` | Show/change manager model within its current provider |
| `/models` | Show role routing |
| `/models worker MODEL [PROVIDER]` | Set every worker role |
| `/models developer MODEL [PROVIDER]` | Set developer routing |
| `/models reviewer MODEL [PROVIDER]` | Set reviewer routing |
| `/models qa MODEL [PROVIDER]` | Set QA routing |
| `/models ROLE inherit` | Reset that role's model/provider to manager inheritance |
| `/multimodel`, `/multimodel on`, `/multimodel off`, `/multimodel status` | Toggle/set/show worker routing |
| **Tab** | Toggle multi-model routing immediately |
| **Ctrl+P**, `/` | Open/filter command picker |
| **Up/Down**, **Enter**, **Esc** | Select, insert/submit, close command picker |
| **PageUp/PageDown** | Browse earlier full-screen output |
| `/team`, `/team on`, `/team off`, `/team status` | Toggle/set/show team mode/progress |
| `/team resume` | Resume/reconcile an interrupted run, outside active work |
| `/stop` | Cancel active team work |
| `/approve y`, `/approve n` | Approve/deny a pending team action |
| `/skills` | List skills and diagnostics |
| `/skill NAME [arguments]`, `$NAME request` | Select a portable skill |
| `/skill clear` | Clear explicit selections |
| `/mcp` | List configured server aliases |

While team work is active, normal text steers the task; `/model`, `/team status`, `/stop`, approval responses and Tab have dedicated live handling. Use other settings commands between runs.

## Complete terminal option reference

Subcommands: `doctor`, `sessions`, `config path`, `config init`, `skills`, `skills show NAME`, `mcp`, `mcp check SERVER`, `mcp tools SERVER`, `mcp call SERVER TOOL`, `mcp read SERVER URI`, `mcp prompt SERVER NAME`, `ai PROMPT`, and `help [COMMAND]`. MCP call/prompt accept `--arguments JSON`; `ai` accepts `--stream`.

| Option | Purpose |
| --- | --- |
| `-h`, `--help` | Help |
| `-V`, `--version` | Version |
| `--cwd DIRECTORY` | Workspace |
| `-p`, `--print` | One turn then exit; prompt from stdin if omitted |
| `--resume`, `--resume=ID` | Latest/specific saved session |
| `--config FILE` | Alternative global configuration |
| `--no-global-config` | Ignore global configuration, retain project settings |
| `--provider NAME` | Manager provider alias |
| `--model MODEL`, `--manager-model MODEL` | Manager/planning/design model |
| `--worker-model MODEL`, `--worker-provider NAME` | Common worker routing |
| `--developer-model MODEL`, `--developer-provider NAME` | Developer overrides |
| `--reviewer-model MODEL`, `--reviewer-provider NAME` | Reviewer overrides |
| `--qa-model MODEL`, `--qa-provider NAME` | QA overrides |
| `--multi-model`, `--no-multi-model` | Enable/disable configured worker routing |
| `--plan`, `--no-plan` | Enable/disable planning |
| `--sudo`, `--no-sudo` | Enable/disable saved design |
| `--design-mode`, `--no-design-mode` | Aliases for sudo switches |
| `--team`, `--no-team` | Team/single-agent implementation |
| `--workers NUMBER` | Concurrent workers, 1–8 |
| `--team-budget NUMBER` | Total calls per team run, 1–1000 |
| `--max-steps NUMBER` | Calls per agent turn, 1–100 |
| `--read-only` | Inspection only |
| `--allow-write` | Preapprove source edits this invocation |
| `--allow-shell` | Preapprove commands this invocation |
| `--allow-mcp` | Preapprove MCP this invocation; read restrictions still apply |
| `--no-mcp` | Disable configured MCP servers |
| `--skill NAME` | Explicit skill; repeat to select multiple |
| `--skill-dir DIRECTORY` | Additional skill location; repeat for multiple paths |
| `--no-skills` | Disable skill discovery/activation |

Environment configuration: `YCODE_CONFIG` selects the global file; `OPENROUTER_API_KEY` supplies the default provider key; `OPENROUTER_MODEL` supplies its environment default model. OpenRouter also supports `OPENROUTER_TIMEOUT_SECS`, `OPENROUTER_MAX_TOKENS`, `OPENROUTER_REFERER` and `OPENROUTER_TITLE` as shown in [.env.example](.env.example). Custom providers reference their own key variables through `key_env`; custom MCP credentials use the fields above.

## Files, troubleshooting and verification

| File/location | Contents |
| --- | --- |
| Global `config.json` | Shared defaults, providers, MCP and skill configuration |
| Project `ycode.config.json` | Project overrides |
| Project `ycode.providers.json` | Standalone provider definitions |
| Project `.mcp.json`, `ycode.mcp.json` | MCP overrides |
| Project `.env` | Optional local provider credentials/settings |
| `.ycode/sessions/` | Saved conversations and tool output |
| `.ycode/plans/SESSION_ID.md` | Latest saved plan for a session |
| `.ycode/design.md` | Human-readable exported design |
| `.ycode/design-context.json` | Canonical design cache |
| `.ycode/teams/RUN_ID/` | Board, coordination comments, transcripts and retained worktrees |

Keep `.ycode/` and credentials private and out of version control. Retained worktrees can use substantial disk space. There is no automatic conversation compaction; use `/clear` for a fresh conversation and keep design context enabled when useful.

| Problem | Next step |
| --- | --- |
| `ycode` not found | Add Cargo bin to PATH; see first-run commands |
| Missing key or wrong model | Run `ycode doctor`; check provider/key environment and replace model placeholders |
| Team cannot start | Check repository root and initial commit, or use `--no-team` |
| Source edits blocked | Check plan/read-only state; use `/implement` or `--no-plan` and approve actions |
| MCP blocked in plan/reviewer mode | Configure exact `read_only_tools`; approval alone does not grant read eligibility |
| Context7 authentication/quota error | Set a Context7 key and `bearer_env`, then retry discovery |
| Skill integration unsupported | Read `ycode skills show NAME` diagnostics; configure MCP separately |
| Interrupted work | Resume the session, inspect partial changes, then `/team resume` if applicable |
| Design stale or refresh failed | Run `/sudo refresh` |

From the harness repository, optional checks:

```powershell
# Local configuration check; no live API request.
ycode doctor

# Live OpenRouter smoke test; uses provider credits.
.\scripts\test_openrouter.ps1 -Agent

# Live Context7 check using the installed binary; no LLM request.
node scripts/verify_context7.mjs "$env:USERPROFILE\.cargo\bin\ycode.exe"

# Source tests and lint checks.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

The Node Context7 check requires Node.js 20.12+ and writes metadata-only results to `target/context7-interoperability/results.json`. It uses an existing Context7 key when present, otherwise anonymous access. See [MCP_COMPATIBILITY.md](MCP_COMPATIBILITY.md) for the separate local reference-server test commands, and [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md) for verified coverage and remaining release limitations.
