# MCP compatibility — October 10, 2026

`ycode` integrates the official Rust MCP SDK (`rmcp 3.5.1`) with bounded transports, permission gates, per-checkout local server connections and command-line diagnostics. This is verified coverage of common protocol features, not a measurement of every MCP server in a registry.

| Surface | Status and evidence |
| --- | --- |
| stdio | Real Node reference filesystem, memory and everything servers; five protocol-version fixtures |
| Streamable HTTP | Real reference everything server; JSON/SSE, stateless/stateful and modern protocol fixtures |
| Deprecated HTTP+SSE | Real reference everything server with a separate event endpoint and same-origin POST endpoint |
| Initialization-era protocol | Fixtures for 2024-11-05, 2025-03-26, 2025-06-18 and 2025-11-25 |
| Modern 2026-07-28 protocol | SDK discovery, per-request metadata and standard routing headers verified against fixtures |
| Tools | Paginated discovery, arbitrary JSON arguments, original names/schemas, structured results, `isError` propagation |
| Resources and templates | Listing, resource reads and templates; real reference resource retrieval |
| Prompts | Listing and string arguments; real reference prompt retrieval; messages returned as untrusted tool data |
| Roots | Server-to-client workspace root requests; Windows file URI normalization; separate worker checkout roots |
| Credentials | Explicit environment references for bearer/custom headers and child environment; inherited LLM credentials excluded |
| Permissions | Separate MCP approval; file/shell grants do not authorize MCP; exact configured read allowlists enforced in plan, reviewer and manager modes |
| Limits and failures | 1 MiB frames/bodies/events, 32 KiB model tool output and arguments, startup/request deadlines, repeated-cursor detection, bounded catalogs, no action replay after timeout |
| Lifecycle | Hidden Windows child windows, Job Object/process-group ownership, cancellation-aware hub shutdown and bounded parallel client close |

The October 10 Windows acceptance run used official `@modelcontextprotocol/server-everything`, `server-filesystem` and `server-memory` **2026.8.31**, with TypeScript SDK **1.32.1**. All three negotiated **2025-11-25**. Eleven real-server checks passed: three stdio catalog checks, echo, filesystem read, memory write/read in an isolated store, resource retrieval, prompt retrieval, Streamable HTTP discovery/call and deprecated SSE discovery/call. Newer protocol behavior is covered by controlled fixtures rather than those older reference servers.

Run these checks again without LLM credentials:

```powershell
npm install --prefix target/mcp-interoperability --ignore-scripts --no-audit --no-fund @modelcontextprotocol/server-everything@2026.8.31 @modelcontextprotocol/server-filesystem@2026.8.31 @modelcontextprotocol/server-memory@2026.8.31 @modelcontextprotocol/sdk@1.32.1
cargo build -p y-cli --locked
node scripts/verify_mcp.mjs
```

Results are written to `target/mcp-interoperability/results.json`. Test data is retained in a separate temporary project; a test-only preload constrains reference HTTP listeners to loopback. CI runs the same reference checks on its Windows/Linux/macOS matrix; only the local Windows result is confirmed here. Node.js 20.12+ is needed for these reference checks (CI uses 22), not for the Rust client itself.

## Context7 live verification

The installed Windows `ycode.exe` passed three live checks against `https://mcp.context7.com/mcp` on October 10, 2026: catalog discovery, `resolve-library-id` and `query-docs`. The service reported Context7 **4.3.0** and negotiated **2026-07-28**, providing a real hosted-server check of the modern protocol in addition to fixtures. The calls resolved React to `/reactjs/react.dev` and retrieved documentation with source links. Both calls ran with `--plan` and the exact read allowlist. No Context7 or LLM API key was present for this run; authenticated Context7 access has not been live-tested.

[ycode.context7.example.json](ycode.context7.example.json) provides a reusable global/project entry. For API-key access add `"bearer_env": "CONTEXT7_API_KEY"` and set the environment variable; this follows the official [Context7 authentication setup](https://context7.com/docs/resources/all-clients). Do not substitute an LLM-provider key. Anonymous availability and quota are service policy, not guaranteed by ycode.

```powershell
node scripts/verify_context7.mjs "$env:USERPROFILE\.cargo\bin\ycode.exe"
```

This optional live test stores metadata-only results in `target/context7-interoperability/results.json`; documentation queries contain only public example questions. It is not part of CI because service availability, credentials and quota vary.

## Compatibility limits

Browser OAuth login/refresh, server-requested sampling, elicitation forms/URLs, MCP Apps UI, subscriptions exposed to users, and long-running task orchestration are not integrated. The client does not advertise sampling, elicitation or app capabilities. Requests requiring interactive follow-up or tasks return a diagnostic instead of automatically invoking models or replaying a tool. Use environment tokens or custom credential headers where the server supports them; OAuth-only servers need a separately managed authorized proxy/token flow.

Image/audio/blob content is retained as bounded JSON tool data, not rendered as a multimodal conversation or interactive app. Large data may exceed the limits. Arbitrary custom transports and host-specific plugin hooks are unsupported. Deprecated SSE has no reconnect/resume mechanism in this stage; reconnect by restarting the invocation after inspecting any partial side effects. The SDK handles protocol negotiation, but automatic HTTP session reinitialization/replay is explicitly disabled. Restart after an expired session; application actions are never automatically replayed by ycode.

`read_only_tools` is a user-maintained exact allowlist. Server `readOnlyHint` annotations do not authorize actions or guarantee absence of writes. Starting a local server can execute arbitrary code even when its tools are subsequently restricted. Approved MCP services have their normal filesystem/network access; roots are context, not an OS sandbox. Fixed filesystem paths in a server's configuration can point outside a worker checkout; use `${YCODE_WORKSPACE}` when checkout-relative behavior is intended. Review MCP approvals accordingly.

Configuration is deliberately selected through ycode's global/project settings or explicit project `.mcp.json` / `ycode.mcp.json`. Foreign application credential stores, browser sessions and plugin caches are not automatically imported. A generic tool bridge (`mcp_list`, `mcp_call`, `mcp_read_resource`, `mcp_get_prompt`) handles arbitrary server tool schemas without putting every tool's full definition into model context. Skills referencing host-specific `mcp__SERVER__TOOL` names must use that bridge to the configured alias and original tool name.

Protocol references: [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk), [current MCP versioning and compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), [initialization-era transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [official reference servers](https://github.com/modelcontextprotocol/servers).
