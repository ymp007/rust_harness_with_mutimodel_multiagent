//! Full-screen terminal workspace; one lock owns transcript and composer output.
use std::{
    collections::VecDeque,
    fmt,
    io::{self, IsTerminal, Write},
    sync::{Mutex, OnceLock},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show commands"),
    ("/implement", "Implement the saved plan"),
    ("/plan on", "Plan with read-only tools"),
    ("/plan off", "Enable implementation"),
    ("/sudo status", "Inspect saved project design"),
    ("/sudo refresh", "Refresh project design"),
    ("/sudo on", "Use saved design context"),
    ("/sudo off", "Disable design context"),
    ("/model", "Show or change manager model"),
    ("/models", "Configure worker models/providers"),
    ("/model ", "Set manager: type MODEL_ID"),
    ("/models worker ", "Set all workers: MODEL [PROVIDER]"),
    ("/models developer ", "Set developer: MODEL [PROVIDER]"),
    ("/models reviewer ", "Set reviewer: MODEL [PROVIDER]"),
    ("/models qa ", "Set QA: MODEL [PROVIDER]"),
    ("/multimodel on", "Use configured worker models"),
    ("/multimodel off", "Use the manager for all roles"),
    ("/team status", "Show worker progress"),
    ("/team on", "Enable project manager/workers"),
    ("/team off", "Use a single agent"),
    ("/team resume", "Reconcile interrupted work"),
    ("/skills", "List portable skills"),
    ("/skill", "Select a skill"),
    ("/skill clear", "Clear selected skills"),
    ("/mcp", "List MCP servers"),
    ("/session", "Show session details"),
    ("/clear", "Start a new conversation"),
    ("/stop", "Stop active team work"),
    ("/approve y", "Approve pending action"),
    ("/approve n", "Deny pending action"),
    ("/exit", "Exit ycode"),
];

pub fn suggestions(text: &str) -> Vec<usize> {
    if !text.starts_with('/') || text.contains('\n') {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(i, (command, _))| command.starts_with(text).then_some(i))
        .collect()
}

#[derive(Default)]
struct State {
    enabled: bool,
    color: bool,
    text: String,
    cursor: usize,
    menu: Vec<usize>,
    selected: usize,
    frame: Vec<String>,
    dimensions: (usize, usize),
    transcript: VecDeque<String>,
    transcript_bytes: usize,
    scroll: usize,
    profile: crate::models::Profile,
    model: String,
    provider: String,
    workspace: String,
    plan: bool,
    design: bool,
    team: bool,
    multi: bool,
    busy: bool,
}
static UI: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> &'static Mutex<State> {
    UI.get_or_init(|| Mutex::new(State::default()))
}
pub fn enabled() -> bool {
    state().lock().unwrap_or_else(|e| e.into_inner()).enabled
}

pub fn configure(ws: &crate::tools::Workspace, session: &crate::session::Session) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.workspace = friendly_path(&ws.root.to_string_lossy());
    ui.model = crate::tools::display(&session.model);
    ui.provider = session
        .manager_provider
        .clone()
        .unwrap_or_else(|| "openrouter".into());
    ui.plan = session.plan_mode;
    ui.design = session.sudo_mode;
    ui.team = session.team_mode;
    ui.multi = session.multi_model;
    ui.profile = session.profile();
    draw(&mut ui);
}

pub fn friendly_path(path: &str) -> String {
    let path = path
        .strip_prefix("\\\\?\\UNC\\")
        .map(|p| format!("\\\\{p}"))
        .unwrap_or_else(|| path.strip_prefix("\\\\?\\").unwrap_or(path).into());
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if !home.is_empty()
        && (path == home
            || path.starts_with(&format!("{home}\\"))
            || path.starts_with(&format!("{home}/")))
    {
        crate::tools::display(&format!("~{}", &path[home.len()..]))
    } else {
        crate::tools::display(&path)
    }
}

pub fn start(native: bool) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.enabled =
        native && io::stderr().is_terminal() && std::env::var("TERM").as_deref() != Ok("dumb");
    ui.color = std::env::var_os("NO_COLOR").is_none();
    if !ui.enabled {
        std::eprintln!(
            "ycode v{} | {}\n{}\nType /help for commands. Submit a task to build design context.",
            env!("CARGO_PKG_VERSION"),
            ui.model,
            ui.workspace
        );
        return;
    }
    let _ = write!(io::stderr(), "\x1b[?1049h\x1b[2J\x1b[H");
    ui.frame.clear();
    draw(&mut ui);
}

pub fn busy(on: bool) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.busy = on;
    draw(&mut ui);
}
pub fn multi(on: bool) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.multi = on;
    draw(&mut ui);
}
pub fn workers(profile: &crate::models::Profile) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.profile = profile.clone();
    ui.multi = profile.enabled;
    draw(&mut ui);
}
pub fn scroll(direction: i32) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    let page = crate::terminal::size().1.saturating_sub(10).max(1);
    ui.scroll = if direction > 0 {
        ui.scroll.saturating_add(page)
    } else {
        ui.scroll.saturating_sub(page)
    };
    draw(&mut ui);
}
pub fn resize() {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    let (columns, height) = crate::terminal::size();
    if ui.dimensions != (columns.clamp(2, 500), height.clamp(1, 200)) {
        draw(&mut ui);
    }
}
pub fn manager(model: &str) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.model = crate::tools::display(model);
    draw(&mut ui);
}
pub fn edit(text: &str, cursor: usize, menu: Vec<usize>, selected: usize) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.text = text.into();
    ui.cursor = cursor;
    ui.menu = menu;
    ui.selected = selected;
    draw(&mut ui);
}
pub fn submit(line: &str) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    ui.text.clear();
    ui.cursor = 0;
    ui.menu.clear();
    ui.scroll = 0;
    if ui.enabled {
        append(&mut ui, &format!("\n› {line}\n"));
    }
    draw(&mut ui);
}
pub fn stop() {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    if ui.enabled {
        let _ = write!(io::stderr(), "\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = io::stderr().flush();
    }
    ui.enabled = false;
    ui.frame.clear();
}

pub fn output(args: fmt::Arguments<'_>, stdout: bool) {
    let mut ui = state().lock().unwrap_or_else(|e| e.into_inner());
    if !ui.enabled {
        if stdout {
            std::println!("{args}");
        } else {
            std::eprintln!("{args}");
        }
        return;
    }
    // Interactive output uses the same console stream as the composer. Piped
    // and machine-readable commands keep their original stdout/stderr split.
    if stdout && !io::stdout().is_terminal() {
        std::println!("{args}");
    } else {
        append(&mut ui, &args.to_string());
    }
    draw(&mut ui);
}

pub fn tool(name: &str, arguments: &str) {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let detail = args
        .get("path")
        .or_else(|| args.get("server"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let label = match name {
        "read_file" => "Read",
        "list_files" => "Inspect",
        "search" => "Search",
        "edit_file" => "Edit",
        "write_file" => "Create",
        "run_command" => "Run command",
        _ => name,
    };
    output(
        format_args!(
            "  ↳ {} {}",
            crate::tools::display(label),
            crate::tools::display(detail)
        ),
        false,
    );
}

macro_rules! eprintln { ($($arg:tt)*) => { crate::ui::output(format_args!($($arg)*), false) }; }
macro_rules! println { () => { crate::ui::output(format_args!(""), true) }; ($($arg:tt)*) => { crate::ui::output(format_args!($($arg)*), true) }; }
pub(crate) use {eprintln, println};

fn clip(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let n = ch.width().unwrap_or(0);
        if used + n > width.saturating_sub(1) {
            break;
        }
        result.push(ch);
        used += n;
    }
    result.push('…');
    result
}

fn append(ui: &mut State, text: &str) {
    let text = crate::tools::display(text).replace('\t', "    ");
    let mut added = 0;
    for line in text.split('\n') {
        let line = clip(line, 32 * 1024);
        if ui.scroll > 0 {
            added += wrap_line(&line, ui.dimensions.0.saturating_sub(1).max(1)).len();
        }
        ui.transcript_bytes += line.len();
        ui.transcript.push_back(line);
    }
    while ui.transcript_bytes > 128 * 1024 || ui.transcript.len() > 4000 {
        if let Some(line) = ui.transcript.pop_front() {
            ui.transcript_bytes -= line.len();
        } else {
            break;
        }
    }
    if ui.scroll > 0 {
        ui.scroll = ui.scroll.saturating_add(added);
    }
}

// A bounded horizontal viewport keeps long prompts and Unicode inside the
// composer without wrapping into the transcript or losing the edit position.
fn viewport(text: &str, cursor: usize, width: usize) -> (String, usize) {
    let shown = text.replace('\n', " ");
    let cursor = cursor.min(shown.len());
    let mut start = 0;
    while UnicodeWidthStr::width(&shown[start..cursor]) >= width.max(1) {
        start += shown[start..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
        if start == cursor {
            break;
        }
    }
    let offset = UnicodeWidthStr::width(&shown[start..cursor]);
    (clip(&shown[start..], width), offset)
}

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let n = ch.width().unwrap_or(0);
        if used + n > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if n > width {
            line.push('?');
            used += 1;
        } else {
            line.push(ch);
            used += n;
        }
    }
    lines.push(line);
    lines
}

fn layout(ui: &mut State, columns: usize, height: usize) -> (Vec<String>, (usize, usize)) {
    let width = columns.saturating_sub(1).max(1);
    let height = height.max(1);
    let mut lines = vec![String::new(); height];
    let (text, cursor) = viewport(&ui.text, ui.cursor, width.saturating_sub(2).max(1));
    if width < 20 || height < 11 {
        lines[0] = clip("ycode · enlarge terminal", width);
        lines[height - 1] = clip(&format!("> {text}"), width);
        return (lines, (height, (cursor + 3).min(width)));
    }
    let accent = if ui.color { "\x1b[96m" } else { "" };
    let dim = if ui.color { "\x1b[90m" } else { "" };
    let reset = if ui.color { "\x1b[0m" } else { "" };
    lines[0] = format!(
        "{accent}{}{reset}",
        clip(
            &format!("ycode v{} · {}", env!("CARGO_PKG_VERSION"), ui.workspace),
            width
        )
    );
    lines[1] = clip(
        &format!("Manager   {} · {}    /model MODEL", ui.provider, ui.model),
        width,
    );
    for (row, (label, model, provider)) in [
        (
            "Developer",
            &ui.profile.developer,
            &ui.profile.developer_provider,
        ),
        (
            "Reviewer",
            &ui.profile.reviewer,
            &ui.profile.reviewer_provider,
        ),
        ("QA", &ui.profile.qa, &ui.profile.qa_provider),
    ]
    .into_iter()
    .enumerate()
    {
        let model = if ui.multi {
            model.as_deref().unwrap_or(&ui.model)
        } else {
            &ui.model
        };
        let provider = if ui.multi {
            provider.as_deref().unwrap_or(&ui.provider)
        } else {
            &ui.provider
        };
        lines[row + 2] = format!(
            "{dim}{}{reset}",
            clip(&format!("{label:<9} {provider} · {model}"), width)
        );
    }
    let composer = height - 5;
    let menu_count = ui.menu.len().min(6).min(composer.saturating_sub(6));
    let transcript_end = composer - menu_count;
    let available = transcript_end.saturating_sub(6);
    if ui.transcript.is_empty() {
        let row = 5 + available / 2;
        if row < transcript_end {
            let label = "y c o d e";
            lines[row] = format!(
                "{accent}{}{label}{reset}",
                " ".repeat(width.saturating_sub(label.len()) / 2)
            );
        }
        for (offset, label) in [
            "Describe a task to begin.",
            "Design builds from inspected files.",
            "/model MODEL · /models worker MODEL",
        ]
        .into_iter()
        .enumerate()
        {
            if row + offset + 1 < transcript_end {
                lines[row + offset + 1] = clip(
                    &format!(
                        "{}{label}",
                        " ".repeat(width.saturating_sub(UnicodeWidthStr::width(label)) / 2)
                    ),
                    width,
                );
            }
        }
    } else {
        let wrapped: Vec<String> = ui
            .transcript
            .iter()
            .flat_map(|line| wrap_line(line, width))
            .collect();
        ui.scroll = ui.scroll.min(wrapped.len().saturating_sub(available));
        let end = wrapped.len().saturating_sub(ui.scroll);
        let start = end.saturating_sub(available);
        for (row, line) in wrapped[start..end].iter().enumerate() {
            lines[6 + row] = line.clone();
        }
    }
    let menu_start = ui.selected.saturating_sub(menu_count.saturating_sub(1));
    for (row, (position, index)) in ui
        .menu
        .iter()
        .enumerate()
        .skip(menu_start)
        .take(menu_count)
        .enumerate()
    {
        let (command, description) = COMMANDS[*index];
        let entry = clip(
            &format!(
                "{} {command:<20} {description}",
                if position == ui.selected { "›" } else { " " }
            ),
            width,
        );
        lines[transcript_end + row] = if position == ui.selected && ui.color {
            format!("\x1b[30;106m{entry}{reset}")
        } else {
            entry
        };
    }
    lines[composer] = format!("{dim}{}{reset}", "─".repeat(width));
    let phase = if ui.plan { "Plan" } else { "Build" };
    let status = format!(
        "{phase}{} · Design {} · {} · {}{}",
        if ui.busy { " / Working" } else { "" },
        if ui.design { "on" } else { "off" },
        if ui.team { "Team" } else { "Single agent" },
        if ui.multi {
            "Multi-model"
        } else {
            "Single model"
        },
        if ui.scroll > 0 {
            " · Earlier output"
        } else {
            ""
        }
    );
    lines[composer + 1] = format!("{accent}{}{reset}", clip(&status, width));
    let input = if ui.text.is_empty() {
        format!("{dim}{}{reset}", clip("Ask about your project…", width - 2))
    } else {
        text
    };
    lines[composer + 2] = format!("{accent}›{reset} {input}");
    lines[composer + 3] = format!("{dim}{}{reset}", "─".repeat(width));
    let hint = if ui.menu.is_empty() {
        "Enter send · / commands · Tab models · PgUp/PgDn history · Ctrl+C exit"
    } else {
        "↑↓ choose · Enter insert · Esc close · Tab models"
    };
    lines[composer + 4] = format!("{dim}{}{reset}", clip(hint, width));
    (lines, (composer + 3, (cursor + 3).min(width)))
}

fn draw(ui: &mut State) {
    if !ui.enabled {
        return;
    }
    let (columns, height) = crate::terminal::size();
    let dimensions = (columns.clamp(2, 500), height.clamp(1, 200));
    if ui.dimensions != dimensions {
        ui.frame.clear();
        ui.dimensions = dimensions;
    }
    let (frame, (row, column)) = layout(ui, dimensions.0, dimensions.1);
    let mut out = io::stderr().lock();
    let _ = write!(out, "\x1b[?25l");
    for (index, line) in frame.iter().enumerate() {
        if ui.frame.get(index) != Some(line) {
            let _ = write!(out, "\x1b[{};1H\x1b[2K{line}", index + 1);
        }
    }
    let _ = write!(out, "\x1b[{row};{column}H\x1b[?25h");
    let _ = out.flush();
    ui.frame = frame;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incoming_output_preserves_the_history_view() {
        let mut ui = State {
            dimensions: (80, 24),
            ..State::default()
        };
        for n in 0..100 {
            append(&mut ui, &format!("line {n}"));
        }
        ui.scroll = 20;
        let before = layout(&mut ui, 80, 24).0;
        append(&mut ui, "incoming worker report");
        assert_eq!(before, layout(&mut ui, 80, 24).0);
    }
    #[test]
    fn full_screen_layout_bounds_roles_composer_and_scrolled_output() {
        let mut ui = State {
            model: "large-manager".into(),
            provider: "manager-api".into(),
            multi: true,
            ..State::default()
        };
        ui.profile.developer = Some("small-worker".into());
        ui.profile.developer_provider = Some("worker-api".into());
        for n in 0..100 {
            append(&mut ui, &format!("line {n} 界 😀"));
        }
        for (width, height) in [(8, 3), (20, 10), (80, 24), (120, 40)] {
            let (frame, (row, column)) = layout(&mut ui, width, height);
            assert_eq!(frame.len(), height);
            assert!(frame
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) < width));
            assert!(row <= height && column < width);
        }
        let (frame, _) = layout(&mut ui, 100, 24);
        assert!(frame[2].contains("small-worker") && frame[2].contains("worker-api"));
        assert!(frame.iter().any(|line| line.contains("line 99")));
        ui.scroll = 20;
        let (frame, _) = layout(&mut ui, 100, 24);
        assert!(!frame.iter().any(|line| line.contains("line 99")));
        ui.multi = false;
        assert!(layout(&mut ui, 100, 24).0[2].contains("large-manager"));
    }
    #[test]
    fn transcript_limits_and_model_entries_are_available_without_a_catalog() {
        let mut ui = State::default();
        for _ in 0..6000 {
            append(&mut ui, &"界".repeat(100));
        }
        assert!(ui.transcript.len() <= 4000 && ui.transcript_bytes <= 128 * 1024);
        assert!(suggestions("/models worker")
            .iter()
            .any(|index| COMMANDS[*index].0 == "/models worker "));
    }
    #[test]
    fn viewport_handles_wide_unicode_and_keeps_cursor_inside() {
        let text = "abc界😀def";
        for cursor in text.char_indices().map(|(n, _)| n).chain([text.len()]) {
            let (view, column) = viewport(text, cursor, 6);
            assert!(UnicodeWidthStr::width(view.as_str()) <= 6);
            assert!(column < 6);
        }
        assert_eq!(clip("界界界", 4), "界…");
    }
    #[test]
    fn command_filter_never_suggests_plain_prompts_or_unknown_commands() {
        assert!(suggestions("explain").is_empty());
        assert!(suggestions("/unknown").is_empty());
        assert!(suggestions("/sudo")
            .iter()
            .all(|i| COMMANDS[*i].0.starts_with("/sudo")));
    }
}
