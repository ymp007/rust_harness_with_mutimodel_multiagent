// This crate owns the only `unsafe` in the workspace: Win32 job-object FFI and
// the Unix process-group kill in `process`. Deny rather than forbid so a new
// unsafe block has to be explicitly justified with an `#[expect]`.
#![deny(unsafe_code)]

mod agent;
mod design;
mod input;
mod mcp;
#[cfg(test)]
mod mcp_tests;
mod mcp_transport;
#[cfg(test)]
mod mode_tests;
#[cfg(test)]
mod model_tests;
mod models;
mod process;
mod providers;
mod session;
mod settings;
#[cfg(test)]
mod skill_tests;
mod skills;
mod team;
mod team_board;
mod team_git;
#[cfg(test)]
mod team_tests;
mod terminal;
#[cfg(test)]
mod tests;
mod tools;
mod ui;

use clap::{Arg, ArgAction, Command};
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use tools::{Permissions, Result, Workspace};
use y_gateway::{OpenRouterClient, OpenRouterConfig};

fn cli() -> Command {
    Command::new("ycode").version(env!("CARGO_PKG_VERSION"))
        .about("Y Code: a multi-provider coding assistant for your terminal")
        .arg(Arg::new("skill-dir").long("skill-dir").global(true).action(ArgAction::Append).value_name("DIRECTORY").help("Additional portable skill package or collection directory"))
        .arg(Arg::new("skill").long("skill").action(ArgAction::Append).value_name("NAME").help("Explicitly activate a skill for this session"))
        .arg(Arg::new("no-skills").long("no-skills").global(true).action(ArgAction::SetTrue).help("Disable skill discovery and activation"))
        .arg(Arg::new("no-mcp").long("no-mcp").global(true).action(ArgAction::SetTrue).help("Disable configured MCP servers for this invocation"))
        .arg(Arg::new("allow-mcp").long("allow-mcp").global(true).action(ArgAction::SetTrue).conflicts_with("no-mcp").help("Approve MCP connections and actions for this invocation; plan/read-only allowlists still apply"))
        .arg(Arg::new("config").long("config").global(true).conflicts_with("no-global-config").value_name("FILE").help("Global configuration file (also YCODE_CONFIG)"))
        .arg(Arg::new("no-global-config").long("no-global-config").global(true).action(ArgAction::SetTrue).help("Ignore user-wide configuration for this invocation"))
        .arg(Arg::new("provider").long("provider").global(true).help("Manager API provider name from ycode.providers.json; default OpenRouter"))
        .arg(Arg::new("worker-provider").long("worker-provider").help("API provider for all worker roles"))
        .arg(Arg::new("developer-provider").long("developer-provider"))
        .arg(Arg::new("reviewer-provider").long("reviewer-provider"))
        .arg(Arg::new("qa-provider").long("qa-provider"))
        .arg(Arg::new("cwd").long("cwd").value_name("DIRECTORY").global(true).help("Workspace directory (default: current directory)"))
        .arg(Arg::new("model").long("model").visible_alias("manager-model").value_name("MODEL").global(true).help("Manager/planning/design model ID (must support tools)"))
        .arg(Arg::new("print").short('p').long("print").action(ArgAction::SetTrue).help("Run one turn and exit; read prompt from stdin when omitted"))
        .arg(Arg::new("resume").long("resume").num_args(0..=1).default_missing_value("last").require_equals(true).value_name("ID").help("Resume latest session, or --resume=ID"))
        .arg(Arg::new("read-only").long("read-only").action(ArgAction::SetTrue).conflicts_with_all(["allow-write", "allow-shell"]).help("Expose only file reads and search"))
        .arg(Arg::new("plan").long("plan").action(ArgAction::SetTrue).conflicts_with("no-plan").help("Inspect and plan without source edits or shell commands; save the plan"))
        .arg(Arg::new("no-plan").long("no-plan").action(ArgAction::SetTrue).help("Disable plan mode (new sessions default to planning)"))
        .arg(Arg::new("sudo").long("sudo").visible_alias("design-mode").action(ArgAction::SetTrue).conflicts_with("no-sudo").help("Use saved pseudocode/design context; update it from prompted source inspection"))
        .arg(Arg::new("no-sudo").long("no-sudo").visible_alias("no-design-mode").action(ArgAction::SetTrue).help("Disable saved design context (enabled by default)"))
        .arg(Arg::new("team").long("team").action(ArgAction::SetTrue).conflicts_with("no-team").help("Use a project manager with developer, reviewer and QA workers (new-session default)"))
        .arg(Arg::new("no-team").long("no-team").action(ArgAction::SetTrue).help("Use the single-agent implementation loop"))
        .arg(Arg::new("workers").long("workers").value_parser(clap::value_parser!(u32).range(1..=8)).help("Maximum concurrent workers (default: 3)"))
        .arg(Arg::new("team-budget").long("team-budget").value_parser(clap::value_parser!(u32).range(1..=1000)).help("Total manager + worker model calls per team run (default: 120)"))
        .arg(Arg::new("developer-model").long("developer-model").value_name("MODEL"))
        .arg(Arg::new("reviewer-model").long("reviewer-model").value_name("MODEL"))
        .arg(Arg::new("qa-model").long("qa-model").value_name("MODEL"))
        .arg(Arg::new("worker-model").long("worker-model").value_name("MODEL").help("Default model for all worker roles; role flags override this"))
        .arg(Arg::new("multi-model").long("multi-model").action(ArgAction::SetTrue).conflicts_with("no-multi-model").help("Enable configured worker models; Tab toggles interactively"))
        .arg(Arg::new("no-multi-model").long("no-multi-model").action(ArgAction::SetTrue).help("Use the manager model for every role, retaining worker settings"))
        .arg(Arg::new("allow-write").long("allow-write").action(ArgAction::SetTrue).help("Approve file edits for this invocation"))
        .arg(Arg::new("allow-shell").long("allow-shell").action(ArgAction::SetTrue).help("Approve shell commands for this invocation (normal OS access)"))
        .arg(Arg::new("max-steps").long("max-steps").default_value("30").value_parser(clap::value_parser!(u32).range(1..=100)).help("Maximum model calls per user turn"))
        .arg(Arg::new("prompt").num_args(1..).value_name("PROMPT"))
        .subcommand(Command::new("doctor").about("Check local configuration without making an API call"))
        .subcommand(Command::new("sessions").about("List saved session IDs in this workspace"))
        .subcommand(Command::new("mcp").about("List configured MCP servers; no LLM API key needed")
            .subcommand(Command::new("check").about("Connect and verify all advertised tools/resources/prompts catalogs").arg(Arg::new("server").required(true)))
            .subcommand(Command::new("tools").about("Discover one MCP server's tools").arg(Arg::new("server").required(true)))
            .subcommand(Command::new("call").about("Call an MCP tool with approval, without an LLM").arg(Arg::new("server").required(true)).arg(Arg::new("tool").required(true)).arg(Arg::new("arguments").long("arguments").default_value("{}").value_name("JSON")))
            .subcommand(Command::new("read").about("Read an MCP resource with approval").arg(Arg::new("server").required(true)).arg(Arg::new("uri").required(true)))
            .subcommand(Command::new("prompt").about("Get an MCP prompt with approval").arg(Arg::new("server").required(true)).arg(Arg::new("name").required(true)).arg(Arg::new("arguments").long("arguments").default_value("{}").value_name("JSON"))))
        .subcommand(Command::new("skills").about("List portable skills and compatibility diagnostics without API calls")
            .subcommand(Command::new("show").about("Show a skill manifest and its compatibility diagnostics").arg(Arg::new("name").required(true))))
        .subcommand(Command::new("config").about("Manage user-wide configuration")
            .subcommand_required(true)
            .subcommand(Command::new("path").about("Show the global configuration path"))
            .subcommand(Command::new("init").about("Create a global template without overwriting an existing file")))
        .subcommand(Command::new("ai").about("Plain chat without repository tools")
            .arg(Arg::new("stream").long("stream").action(ArgAction::SetTrue))
            .arg(Arg::new("prompt").num_args(1..).required(true)))
        .after_help("Examples:\n  ycode\n  ycode \"Explain this project\"\n  ycode -p --read-only \"Review the error handling\"\n  ycode --resume\n  ycode --plan --sudo \"Plan a parser fix\"\n  ycode ai --stream \"Say hello\"\n\nNew sessions default to plan + saved design + team workers. Use /implement to continue a plan and /model to switch models. Tab toggles multi-model; /models configures worker roles. Use ycode mcp check NAME to verify configured MCP servers without an LLM key. Use ycode skills to inspect portable packages and --skill NAME to activate one. Use --provider NAME with ycode.providers.json for custom APIs. File edits and commands require approval; non-interactive input denies unapproved actions.")
}

fn prompt(matches: &clap::ArgMatches) -> String {
    matches
        .get_many::<String>("prompt")
        .map(|values| values.cloned().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

fn selected_provider(
    ws: &Workspace,
    matches: &clap::ArgMatches,
    settings: &settings::Settings,
) -> Result<Option<String>> {
    if let Some(provider) = matches.get_one::<String>("provider") {
        return Ok(Some(provider.clone()));
    }
    if let Some(id) = matches.get_one::<String>("resume") {
        return Ok(session::Session::load(ws, id)?.manager_provider);
    }
    Ok(settings.defaults.provider.clone())
}
fn config(
    ws: &Workspace,
    matches: &clap::ArgMatches,
    settings: &settings::Settings,
) -> Result<OpenRouterConfig> {
    let mut cfg = if selected_provider(ws, matches, settings)?.is_some() {
        let model = matches
            .get_one::<String>("model")
            .cloned()
            .or_else(|| {
                matches
                    .get_one::<String>("resume")
                    .and_then(|id| session::Session::load(ws, id).ok().map(|s| s.model))
            })
            .or_else(|| settings.defaults.model.clone())
            .ok_or("Select --model MODEL for the configured provider")?;
        OpenRouterConfig {
            api_key: String::new(),
            model,
            referer: String::new(),
            title: "Y Code".into(),
            timeout_secs: 300,
            max_tokens: 4096,
        }
    } else {
        OpenRouterConfig::from_env()?
    };
    if let Some(model) = matches.get_one::<String>("model") {
        cfg.model = model.clone();
    } else if let Some(id) = matches.get_one::<String>("resume") {
        cfg.model = session::Session::load(ws, id)?.model;
    } else if let Some(model) = &settings.defaults.model {
        cfg.model = model.clone();
    }
    if cfg.model.trim().is_empty() {
        return Err("Model must not be empty".into());
    }
    Ok(cfg)
}

async fn run() -> Result<()> {
    let matches = cli().get_matches();
    let cwd = matches
        .get_one::<String>("cwd")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let mut ws = Workspace::new(&cwd)?;
    if let Some(("config", command)) = matches.subcommand() {
        let path = settings::path(&matches)?;
        if command.subcommand_name() == Some("init") {
            settings::init(&path)?;
            crate::ui::println!("Created configuration template. Set your model IDs and credential environment variables.");
        }
        crate::ui::println!("{}", path.display());
        return Ok(());
    }
    y_gateway::load_dotenv(ws.root.join(".env"))?;
    let mut settings = settings::Settings::load(&ws, &matches)?;
    if matches.get_flag("no-mcp") {
        settings.mcp_servers.clear();
    }
    ws.mcp = mcp::Hub::new(
        ws.root.clone(),
        settings.mcp_servers.clone(),
        matches.get_flag("allow-mcp"),
    );
    if let Some(("mcp", command)) = matches.subcommand() {
        if let Some((operation, args)) = command.subcommand() {
            let server = args
                .get_one::<String>("server")
                .ok_or("Missing MCP server name")?;
            let permissions = Permissions {
                read_only: matches.get_flag("read-only")
                    || matches.get_flag("plan")
                    || matches!(operation, "check" | "tools"),
                allow_write: false,
                allow_shell: false,
            };
            let mut params = serde_json::json!({"kind":"tools"});
            if let Some(arguments) = args.try_get_one::<String>("arguments").ok().flatten() {
                if arguments.len() > 32 * 1024 {
                    return Err("MCP arguments exceed 32 KiB".into());
                }
                params["arguments"] = serde_json::from_str(arguments)
                    .map_err(|_| "MCP arguments need a JSON object")?;
                if !params["arguments"].is_object() {
                    return Err("MCP arguments need a JSON object".into());
                }
            }
            for (key, arg) in [("tool", "tool"), ("uri", "uri"), ("prompt", "name")] {
                if let Some(value) = args.try_get_one::<String>(arg).ok().flatten() {
                    params[key] = serde_json::json!(value);
                }
            }
            let action = match operation {
                "check" => "check",
                "tools" => "list",
                "call" => "call",
                "read" => "read",
                "prompt" => "prompt",
                _ => unreachable!(),
            };
            let result = ws.mcp.request(&permissions, server, action, &params).await;
            match result {
                Ok(result) => {
                    let failed = result["isError"].as_bool() == Some(true);
                    crate::ui::println!(
                        "{}",
                        tools::display(&y_gateway::openrouter::redact_secrets(
                            &serde_json::to_string_pretty(&result)?
                        ))
                    );
                    if failed {
                        return Err("MCP tool reported isError=true; see the result above".into());
                    }
                }
                Err(e) => return Err(y_gateway::openrouter::redact_secrets(&e.to_string()).into()),
            }
        } else {
            crate::ui::println!("{}", serde_json::to_string_pretty(&ws.mcp.describe())?);
        }
        return Ok(());
    }
    if matches.get_flag("no-skills") {
        settings.skills.enabled = Some(false);
    }
    let extra = matches
        .get_many::<String>("skill-dir")
        .into_iter()
        .flatten()
        .map(PathBuf::from);
    ws.skills = std::sync::Arc::new(skills::Catalog::discover(&skills::roots(
        &ws,
        &settings.skills,
        extra,
    )));
    if let Some(("skills", command)) = matches.subcommand() {
        if let Some(("show", show)) = command.subcommand() {
            let name = show.get_one::<String>("name").ok_or("Missing skill name")?;
            let skill = ws.skills.resolve(name)?;
            let selected = [skills::Invocation {
                id: skill.id.clone(),
                arguments: String::new(),
            }];
            crate::ui::println!("{}", tools::display(&ws.skills.load(name, "", &selected)?));
        } else {
            crate::ui::println!("{}", tools::display(&skills::list(&ws)?));
        }
        return Ok(());
    }
    if matches.subcommand_name() == Some("doctor") {
        crate::ui::println!(
            "ycode {}\nWorkspace: {}",
            env!("CARGO_PKG_VERSION"),
            ws.root.display()
        );
        match config(&ws, &matches, &settings) {
            Ok(cfg) => {
                let mut client =
                    OpenRouterClient::new(cfg.clone())?.with_routes(settings.providers.clone());
                if let Some(name) = selected_provider(&ws, &matches, &settings)? {
                    client = client.with_provider(&name)?;
                }
                client.with_model(&cfg.model)?;
                crate::ui::println!(
                    "Provider: {} (key redacted)\nModel: {}\nTimeout: {}s\nMax output tokens: {}",
                    selected_provider(&ws, &matches, &settings)?.unwrap_or("OpenRouter".into()),
                    tools::display(&cfg.model),
                    cfg.timeout_secs,
                    cfg.max_tokens
                );
            }
            Err(e) => {
                return Err(e);
            }
        }
        crate::ui::println!("File edits/commands: approval required by default\nLive API: not tested (run ycode ai \"Say hello\")");
        return Ok(());
    }
    if matches.subcommand_name() == Some("sessions") {
        for id in session::list(&ws)? {
            crate::ui::println!("{id}");
        }
        return Ok(());
    }
    let mut cfg = config(&ws, &matches, &settings)?;
    if let Some(("ai", ai)) = matches.subcommand() {
        let mut client =
            OpenRouterClient::new(cfg.clone())?.with_routes(settings.providers.clone());
        if let Some(name) = selected_provider(&ws, &matches, &settings)? {
            client = client.with_provider(&name)?;
        }
        let messages = [serde_json::json!({"role":"user","content":prompt(ai)})];
        let text = if ai.get_flag("stream") {
            client
                .stream(&messages, |piece| {
                    print!("{}", tools::display(piece));
                    let _ = io::stdout().flush();
                })
                .await?;
            crate::ui::println!();
            return Ok(());
        } else {
            client.complete(&messages, &[]).await?["content"]
                .as_str()
                .ok_or("Provider returned no text")?
                .to_owned()
        };
        crate::ui::println!("{}", tools::display(&text));
        return Ok(());
    }
    let _workspace_lock = session::acquire_lock(&ws)?;
    let permissions = Permissions {
        read_only: matches.get_flag("read-only"),
        allow_write: matches.get_flag("allow-write"),
        allow_shell: matches.get_flag("allow-shell"),
    };
    let max_steps =
        if matches.value_source("max-steps") == Some(clap::parser::ValueSource::CommandLine) {
            *matches
                .get_one::<u32>("max-steps")
                .ok_or("Missing max-steps")?
        } else {
            settings.defaults.max_steps.unwrap_or(30)
        } as usize;
    let interactive = io::stdin().is_terminal() && !matches.get_flag("print");
    let mut session = if let Some(id) = matches.get_one::<String>("resume") {
        session::Session::load(&ws, id)?
    } else {
        session::Session::new(&ws, &cfg.model)?
    };
    if settings.skills.enabled != Some(false) {
        for invocation in &session.active_skills {
            skills::activate(&ws, &invocation.id, &invocation.arguments)?;
        }
    }
    for name in matches.get_many::<String>("skill").into_iter().flatten() {
        skills::activate(&ws, name, "")?;
    }
    if settings.skills.enabled != Some(false) {
        session.active_skills = ws
            .active_skills
            .lock()
            .map_err(|_| "Skill state poisoned")?
            .clone();
    }
    if matches.get_one::<String>("resume").is_some() {
        if let Some(on) = models::load_mode(&ws, &session.id)? {
            session.multi_model = on;
        }
    }
    if matches.get_one::<String>("resume").is_some() && matches.get_one::<String>("model").is_none()
    {
        cfg.model = session.model.clone();
    } else {
        session.model = cfg.model.clone();
    }
    if matches.get_one::<String>("resume").is_none() {
        settings.apply(&mut session);
    }
    apply_modes(&matches, &mut session, false);
    session.manager_provider = selected_provider(&ws, &matches, &settings)?;
    let mut client = OpenRouterClient::new(cfg)?
        .with_routes(settings.providers.clone())
        .with_model(&session.model)?;
    if let Some(name) = &session.manager_provider {
        client = client.with_provider(name)?;
    }
    for model in [
        &session.developer_model,
        &session.reviewer_model,
        &session.qa_model,
    ]
    .into_iter()
    .flatten()
    {
        client.with_model(model)?;
    }
    if interactive {
        session.save(&ws)?;
        input::configure_profile(&ws, &session.id, session.multi_model)?;
        ui::configure(&ws, &session);
        input::start();
    }
    let mut input = prompt(&matches);
    if !interactive {
        if input.is_empty() {
            let bytes = tokio::task::spawn_blocking(|| {
                let mut bytes = Vec::new();
                io::stdin().take(100_001).read_to_end(&mut bytes)?;
                Ok::<_, io::Error>(bytes)
            })
            .await??;
            if bytes.len() > 100_000 {
                return Err("Stdin prompt exceeds 100 KiB".into());
            }
            input = String::from_utf8(bytes)?;
        }
        let result = agent::turn(&client, &ws, &mut session, &permissions, &input, max_steps).await;
        session.repair_pending();
        session.save(&ws)?;
        models::save_mode(&ws, &session.id, session.multi_model)?;
        crate::ui::println!("{}", tools::display(&result?));
        return Ok(());
    }
    session.save(&ws)?;
    loop {
        ui::configure(&ws, &session);
        if input.is_empty() {
            match crate::input::line().await {
                Some(line) => input = line,
                None => break,
            }
        }
        let trimmed = input.trim();
        ui::busy(true);
        match trimmed {
            "/exit" | "/quit" => break,
            "/skills" => crate::ui::eprintln!("{}",tools::display(&skills::list(&ws)?)),
            "/mcp" => crate::ui::eprintln!("{}",serde_json::to_string_pretty(&ws.mcp.describe())?),
            "/skill clear" => {ws.active_skills.lock().map_err(|_|"Skill state poisoned")?.clear();session.active_skills.clear();session.save(&ws)?;crate::ui::eprintln!("Explicit skills cleared");},
            "/help" => crate::ui::eprintln!("/help /exit /clear /session\n/mcp lists configured servers. MCP actions require separate approval; --allow-mcp preapproves this invocation.\n/skills lists portable packages and compatibility diagnostics. /skill NAME [arguments] or $NAME selects a skill; /skill clear removes selections.\nTab toggles configured worker models and providers immediately. /multimodel [on|off|status] is the command equivalent.\n/models [worker|developer|reviewer|qa MODEL [PROVIDER]] configures workers; inherit resets to the manager.\n/team [on|off|status|resume] controls project-manager workers.\nDuring team work: type additional direction, /model MODEL, /team status, or /stop.\nApprove actions with /approve y or deny with /approve n.\n/plan [on|off] toggles read-only planning; plans are saved automatically.\n/implement [direction] implements the latest saved plan with normal approvals.\n/model [MODEL_ID] shows or changes the current model without losing context.\n/sudo [on|off|refresh|status] toggles saved pseudocode/design context. /design is an alias.\nNew sessions default to plan + sudo + team. Team implementation requires a Git repository root with an initial commit. Changed source refreshes the design after completed turns."),
            "/session" => crate::ui::eprintln!("Session: {} | Model: {} | Messages: {} | plan={} sudo={}", session.id, tools::display(&session.model), session.messages.len(), session.plan_mode, session.sudo_mode),
            "/clear" => {
                let (plan, sudo) = (session.plan_mode, session.sudo_mode);
                let mut next = session::Session::new(&ws, &session.model)?;
                next.team_mode = session.team_mode; next.worker_limit = session.worker_limit; next.team_budget = session.team_budget;
                next.developer_model = session.developer_model.clone(); next.reviewer_model = session.reviewer_model.clone(); next.qa_model = session.qa_model.clone();
                next.multi_model=session.multi_model;
                next.manager_provider=session.manager_provider.clone();next.set_profile(session.profile());
                session = next;
                ws.active_skills.lock().map_err(|_|"Skill state poisoned")?.clear();
                session.plan_mode = plan; session.sudo_mode = sudo;
                session.save(&ws)?; crate::ui::eprintln!("New session: {}", session.id);
                input::configure_profile(&ws,&session.id,session.multi_model)?;
            }
            "" => (),
            command if command=="/multimodel" || command.starts_with("/multimodel ")=>{
                let action=command.strip_prefix("/multimodel").unwrap_or("").trim();
                if action!="status" {
                    match toggle(action,session.multi_model) {Ok(on)=>{input::set_profile(on)?;session.multi_model=on;session.save(&ws)?;},Err(e)=>crate::ui::eprintln!("{e}")}
                }
                if action == "status" {
                    crate::ui::eprintln!("{}",tools::display(&session.profile().status(&session.model)));
                } else {
                    crate::ui::eprintln!("Multi-model {}", if session.multi_model { "on" } else { "off" });
                }
            }
            command if command=="/models" || command.starts_with("/models ")=>{
                let action=command.strip_prefix("/models").unwrap_or("").trim();
                if !action.is_empty() {
                    let mut profile=session.profile();
                    match profile.configure(&client,action) {Ok(())=>{session.set_profile(profile);session.save(&ws)?;},Err(e)=>crate::ui::eprintln!("{e}")}
                }
                crate::ui::eprintln!("{}",tools::display(&session.profile().status(&session.model)));
            }
            command if command == "/team" || command.starts_with("/team ") => {
                let action = command.strip_prefix("/team").unwrap_or("").trim();
                match action {
                    "resume" => {
                        session.plan_mode = false;
                        match team::run(&client,&ws,&mut session,&permissions,"Resume this project: inspect interrupted checkouts; do not automatically replay commands or edits.",max_steps,true).await {
                            Ok(text) => crate::ui::println!("{}", tools::display(&text)),
                            Err(e) => crate::ui::eprintln!("Team: {}", tools::display(&e.to_string())),
                        }
                    }
                    "status" => {
                        if let Some(id) = &session.active_team { match team_board::Board::load(&ws,id) { Ok(board) => crate::ui::eprintln!("{}", tools::display(&serde_json::to_string_pretty(&board.tasks)?)), Err(e) => crate::ui::eprintln!("{e}") } }
                        else { crate::ui::eprintln!("No team run yet"); }
                    }
                    _ => match toggle(action,session.team_mode) { Ok(on) => { session.team_mode = on; crate::ui::eprintln!("Team mode: {on}"); }, Err(e) => crate::ui::eprintln!("{e}") },
                }
                session.save(&ws)?;
            }
            command if command == "/model" || command.starts_with("/model ") => {
                let model = command.strip_prefix("/model").unwrap_or("").trim();
                if !model.is_empty() {
                    if let Err(e) = switch_model(&mut client, &ws, &mut session, model) {
                        crate::ui::eprintln!("Model change failed: {}", tools::display(&e.to_string()));
                    }
                }
                crate::ui::eprintln!("Model: {}", tools::display(&session.model));
            }
            command if command == "/implement" || command.starts_with("/implement ") => {
                let details = command.strip_prefix("/implement").unwrap_or("").trim();
                match agent::implement(&client, &ws, &mut session, &permissions, details, max_steps).await {
                    Ok(text) => crate::ui::println!("\n{}", tools::display(&text)),
                    Err(e) => crate::ui::eprintln!("Error: {}", tools::display(&e.to_string())),
                }
                session.repair_pending(); session.save(&ws)?;
            }
            command if command == "/plan" || command.starts_with("/plan ") => {
                match toggle(command.strip_prefix("/plan").unwrap_or("").trim(), session.plan_mode) {
                    Ok(enabled) => { session.plan_mode = enabled; session.save(&ws)?; crate::ui::eprintln!("Plan mode: {}", if enabled { "on (source edits and commands disabled)" } else { "off" }); }
                    Err(e) => crate::ui::eprintln!("{e}"),
                }
            }
            command if command == "/sudo" || command.starts_with("/sudo ") || command == "/design" || command.starts_with("/design ") => {
                let action = command.split_once(' ').map(|(_, action)| action.trim()).unwrap_or("");
                let result = design_command(&client, &ws, &mut session, action, max_steps).await;
                if let Err(e) = result { crate::ui::eprintln!("Design mode error: {}", tools::display(&e.to_string())); }
            }
            _ => {
                match agent::turn(&client, &ws, &mut session, &permissions, trimmed, max_steps).await {
                    Ok(text) => crate::ui::println!("\n{}", tools::display(&text)),
                    Err(e) => crate::ui::eprintln!("Error: {}", tools::display(&e.to_string())),
                }
                session.repair_pending(); session.save(&ws)?;
            }
        }
        ui::busy(false);
        input.clear();
    }
    ui::stop();
    Ok(())
}

fn apply_modes(matches: &clap::ArgMatches, session: &mut session::Session, fresh: bool) {
    if fresh {
        session.plan_mode = true;
        session.sudo_mode = true;
        session.team_mode = true;
    }
    if matches.get_flag("plan") {
        session.plan_mode = true;
    }
    if matches.get_flag("no-plan") {
        session.plan_mode = false;
    }
    if matches.get_flag("sudo") {
        session.sudo_mode = true;
    }
    if matches.get_flag("no-sudo") {
        session.sudo_mode = false;
    }
    if matches.get_flag("team") {
        session.team_mode = true;
    }
    if matches.get_flag("no-team") {
        session.team_mode = false;
    }
    if let Some(workers) = matches.get_one::<u32>("workers") {
        session.worker_limit = *workers as usize;
    }
    if let Some(budget) = matches.get_one::<u32>("team-budget") {
        session.team_budget = *budget as usize;
    }
    if let Some(model) = matches.get_one::<String>("developer-model") {
        session.developer_model = Some(model.clone());
    }
    if let Some(model) = matches.get_one::<String>("reviewer-model") {
        session.reviewer_model = Some(model.clone());
    }
    if let Some(model) = matches.get_one::<String>("qa-model") {
        session.qa_model = Some(model.clone());
    }
    if let Some(model) = matches.get_one::<String>("worker-model") {
        if matches.get_one::<String>("developer-model").is_none() {
            session.developer_model = Some(model.clone());
        }
        if matches.get_one::<String>("reviewer-model").is_none() {
            session.reviewer_model = Some(model.clone());
        }
        if matches.get_one::<String>("qa-model").is_none() {
            session.qa_model = Some(model.clone());
        }
    }
    for role in ["developer", "reviewer", "qa"] {
        let provider = matches
            .get_one::<String>(&format!("{role}-provider"))
            .or_else(|| matches.get_one::<String>("worker-provider"));
        if let Some(provider) = provider {
            match role {
                "developer" => session.developer_provider = Some(provider.clone()),
                "reviewer" => session.reviewer_provider = Some(provider.clone()),
                _ => session.qa_provider = Some(provider.clone()),
            }
            session.multi_model = true;
        }
    }
    if [
        "worker-model",
        "developer-model",
        "reviewer-model",
        "qa-model",
    ]
    .iter()
    .any(|key| matches.get_one::<String>(key).is_some())
        || matches.get_flag("multi-model")
    {
        session.multi_model = true;
    }
    if matches.get_flag("no-multi-model") {
        session.multi_model = false;
    }
}

fn toggle(action: &str, enabled: bool) -> Result<bool> {
    match action {
        "" => Ok(!enabled),
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("Expected on or off (omit to toggle)".into()),
    }
}

fn switch_model(
    client: &mut OpenRouterClient,
    ws: &Workspace,
    session: &mut session::Session,
    model: &str,
) -> Result<()> {
    let next = client.with_model(model)?;
    let previous = std::mem::replace(&mut session.model, model.into());
    if let Err(e) = session.save(ws) {
        session.model = previous;
        return Err(e);
    }
    *client = next;
    Ok(())
}

async fn design_command(
    client: &OpenRouterClient,
    ws: &Workspace,
    session: &mut session::Session,
    action: &str,
    max_steps: usize,
) -> Result<()> {
    match action {
        "refresh" => {
            agent::refresh_design(client, ws, &session.model, max_steps).await?;
        }
        "status" => {
            let context = design::DesignContext::load(ws)?;
            crate::ui::eprintln!("Sudo/design mode: {} | .ycode/design.md | {} bytes | {} file changes since generation", session.sudo_mode, context.summary.len(), context.changes(ws)?.len());
        }
        _ => {
            let enabled = toggle(action, session.sudo_mode)?;
            if enabled {
                design::DesignContext::optional(ws)?;
            }
            session.sudo_mode = enabled;
            crate::ui::eprintln!(
                "Sudo/design mode: {}",
                if enabled {
                    "on (saved design + targeted source reads)"
                } else {
                    "off"
                }
            );
        }
    }
    session.save(ws)
}

#[tokio::main]
async fn main() {
    let code = {
        let work = run();
        tokio::pin!(work);
        tokio::select! {
            result = &mut work => match result { Ok(()) => 0, Err(e) => { crate::ui::eprintln!("ycode: {}", tools::display(&e.to_string())); 1 } },
            _ = tokio::signal::ctrl_c() => { crate::ui::eprintln!("\nCancelled. Progress saved; use --resume. Interrupted actions may have side effects."); 130 }
        }
    }; // Drop the work future and its process-tree guard before exit.
    mcp::shutdown_all().await;
    ui::stop();
    terminal::restore();
    std::process::exit(code);
}
