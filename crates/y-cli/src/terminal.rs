//! Minimal native key reader; terminal modes are restored before process::exit.
use std::{
    io,
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))] // Unix-only paste/escape events share the editor enum.
pub enum Key {
    Character(char),
    Tab,
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Escape,
    Palette,
    PageUp,
    PageDown,
    Resize,
    Home,
    End,
    Clear,
    Eof,
    PasteStart,
    PasteEnd,
    Ignore,
}

#[cfg(windows)]
struct State {
    input: usize,
    input_mode: u32,
    output: Option<(usize, u32)>,
}
#[cfg(unix)]
struct State {
    original: libc::termios,
}
static STATE: OnceLock<Mutex<Option<State>>> = OnceLock::new();
pub struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

#[cfg(windows)]
pub fn start() -> io::Result<Guard> {
    use windows_sys::Win32::System::Console::*;
    // SAFETY: borrowed console handles remain owned by the OS. Mode reads/writes
    // use valid handles; original modes are retained and restored exactly once.
    #[expect(unsafe_code, reason = "native Windows console key input")]
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        let mut input_mode = 0;
        if GetConsoleMode(input, &mut input_mode) == 0 {
            return Err(io::Error::last_os_error());
        }
        let output = GetStdHandle(STD_ERROR_HANDLE);
        let mut output_mode = 0;
        let saved_output = if GetConsoleMode(output, &mut output_mode) != 0
            && SetConsoleMode(output, output_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        {
            Some((output as usize, output_mode))
        } else {
            None
        };
        if SetConsoleMode(
            input,
            (input_mode
                & !(ENABLE_LINE_INPUT
                    | ENABLE_ECHO_INPUT
                    | ENABLE_QUICK_EDIT_MODE
                    | ENABLE_VIRTUAL_TERMINAL_INPUT))
                | ENABLE_EXTENDED_FLAGS
                | ENABLE_WINDOW_INPUT
                | ENABLE_PROCESSED_INPUT,
        ) == 0
        {
            if let Some((handle, mode)) = saved_output {
                SetConsoleMode(handle as _, mode);
            }
            return Err(io::Error::last_os_error());
        }
        *STATE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| io::Error::other("Terminal state poisoned"))? = Some(State {
            input: input as usize,
            input_mode,
            output: saved_output,
        });
    }
    Ok(Guard)
}

#[cfg(unix)]
pub fn start() -> io::Result<Guard> {
    // SAFETY: termios storage is initialized by tcgetattr and applies only to stdin.
    #[expect(unsafe_code, reason = "native Unix terminal key input")]
    unsafe {
        let mut original: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut original) != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut mode = original;
        mode.c_lflag &= !(libc::ICANON | libc::ECHO);
        mode.c_iflag &= !(libc::IXON | libc::ICRNL);
        mode.c_cc[libc::VMIN] = 1;
        mode.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(0, libc::TCSANOW, &mode) != 0 {
            return Err(io::Error::last_os_error());
        }
        *STATE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| io::Error::other("Terminal state poisoned"))? = Some(State { original });
    }
    eprint!("\x1b[?2004h");
    Ok(Guard)
}

pub fn restore() {
    let Some(state) = STATE
        .get()
        .and_then(|s| s.lock().ok().and_then(|mut s| s.take()))
    else {
        return;
    };
    #[cfg(windows)]
    // SAFETY: restores borrowed console handles saved by start; no ownership transfer.
    #[expect(unsafe_code, reason = "restore Windows console modes")]
    unsafe {
        use windows_sys::Win32::System::Console::SetConsoleMode;
        SetConsoleMode(state.input as _, state.input_mode);
        if let Some((handle, mode)) = state.output {
            SetConsoleMode(handle as _, mode);
        }
    }
    #[cfg(unix)]
    // SAFETY: restores the previously captured termios for the same stdin fd.
    #[expect(unsafe_code, reason = "restore Unix terminal mode")]
    unsafe {
        libc::tcsetattr(0, libc::TCSANOW, &state.original);
        eprint!("\x1b[?2004l");
    }
}

pub struct Reader {
    #[cfg(windows)]
    surrogate: Option<u16>,
}
impl Reader {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            surrogate: None,
        }
    }
}

#[cfg(windows)]
impl Reader {
    pub fn read(&mut self) -> io::Result<Key> {
        use windows_sys::Win32::System::Console::*;
        loop {
            // SAFETY: INPUT_RECORD is initialized before ReadConsoleInputW writes
            // one record; its union is inspected only for a KEY_EVENT record.
            #[expect(unsafe_code, reason = "read native Windows console key events")]
            unsafe {
                let mut record: INPUT_RECORD = std::mem::zeroed();
                let mut count = 0;
                if ReadConsoleInputW(GetStdHandle(STD_INPUT_HANDLE), &mut record, 1, &mut count)
                    == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if count > 0 && record.EventType == WINDOW_BUFFER_SIZE_EVENT as u16 {
                    return Ok(Key::Resize);
                }
                if count == 0 || record.EventType != KEY_EVENT as u16 {
                    continue;
                }
                let key = record.Event.KeyEvent;
                if key.bKeyDown == 0 {
                    continue;
                }
                let special = match key.wVirtualKeyCode {
                    9 => Some(Key::Tab),
                    13 => Some(Key::Enter),
                    8 => Some(Key::Backspace),
                    46 => Some(Key::Delete),
                    37 => Some(Key::Left),
                    39 => Some(Key::Right),
                    38 => Some(Key::Up),
                    40 => Some(Key::Down),
                    27 => Some(Key::Escape),
                    33 => Some(Key::PageUp),
                    34 => Some(Key::PageDown),
                    36 => Some(Key::Home),
                    35 => Some(Key::End),
                    _ => None,
                };
                if let Some(key) = special {
                    self.surrogate = None;
                    return Ok(key);
                }
                let unit = key.uChar.UnicodeChar;
                if unit == 16 {
                    return Ok(Key::Palette);
                }
                if unit == 4 || unit == 26 {
                    return Ok(Key::Eof);
                }
                if unit == 21 {
                    return Ok(Key::Clear);
                }
                if (0xd800..=0xdbff).contains(&unit) {
                    self.surrogate = Some(unit);
                    continue;
                }
                let point = if (0xdc00..=0xdfff).contains(&unit) {
                    match self.surrogate.take() {
                        Some(high) => {
                            0x10000 + (((high as u32 - 0xd800) << 10) | (unit as u32 - 0xdc00))
                        }
                        None => continue,
                    }
                } else {
                    self.surrogate = None;
                    unit as u32
                };
                if let Some(ch) = char::from_u32(point) {
                    if !ch.is_control() {
                        return Ok(Key::Character(ch));
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
impl Reader {
    pub fn read(&mut self) -> io::Result<Key> {
        use std::io::Read;
        let mut input = io::stdin();
        // SAFETY: poll receives one initialized stdin descriptor. A short idle
        // timeout lets the UI observe Unix terminal resizes without another key.
        #[expect(unsafe_code, reason = "native terminal resize polling")]
        let ready = unsafe {
            let mut fd = libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut fd, 1, 250)
        };
        if ready == 0 {
            return Ok(Key::Resize);
        }
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(Key::Resize);
            }
            return Err(error);
        }
        let mut first = [0];
        if input.read(&mut first)? == 0 {
            return Ok(Key::Eof);
        }
        Ok(match first[0] {
            9 => Key::Tab,
            10 | 13 => Key::Enter,
            8 | 127 => Key::Backspace,
            4 => Key::Eof,
            21 => Key::Clear,
            16 => Key::Palette,
            27 => {
                let mut sequence = Vec::new();
                for _ in 0..6 {
                    // SAFETY: poll receives one initialized fd record for stdin.
                    #[expect(unsafe_code, reason = "bounded escape-sequence input wait")]
                    let ready = unsafe {
                        let mut fd = libc::pollfd {
                            fd: 0,
                            events: libc::POLLIN,
                            revents: 0,
                        };
                        libc::poll(&mut fd, 1, 50) > 0
                    };
                    if !ready {
                        break;
                    }
                    let mut byte = [0];
                    input.read_exact(&mut byte)?;
                    sequence.push(byte[0]);
                    if byte[0].is_ascii_alphabetic() || byte[0] == b'~' {
                        break;
                    }
                }
                match sequence.as_slice() {
                    b"[D" => Key::Left,
                    b"[C" => Key::Right,
                    b"[A" => Key::Up,
                    b"[B" => Key::Down,
                    b"" => Key::Escape,
                    b"[H" | b"[1~" => Key::Home,
                    b"[F" | b"[4~" => Key::End,
                    b"[3~" => Key::Delete,
                    b"[5~" => Key::PageUp,
                    b"[6~" => Key::PageDown,
                    b"[200~" => Key::PasteStart,
                    b"[201~" => Key::PasteEnd,
                    _ => Key::Ignore,
                }
            }
            byte => {
                let length = if byte < 128 {
                    1
                } else if byte & 0xe0 == 0xc0 {
                    2
                } else if byte & 0xf0 == 0xe0 {
                    3
                } else if byte & 0xf8 == 0xf0 {
                    4
                } else {
                    return Ok(Key::Ignore);
                };
                let mut bytes = vec![byte; length];
                if length > 1 {
                    input.read_exact(&mut bytes[1..])?;
                }
                match std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|s| s.chars().next())
                {
                    Some(ch) if !ch.is_control() => Key::Character(ch),
                    _ => Key::Ignore,
                }
            }
        })
    }
}

#[derive(Default)]
pub struct Editor {
    text: String,
    cursor: usize,
    paste: bool,
    selected: usize,
    dismiss_menu: bool,
    disable_picker: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Toggle,
    Submit(String),
    Eof,
    Scroll(i32),
    Resize,
}
impl Editor {
    pub fn set_palette_enabled(&mut self, enabled: bool) {
        self.disable_picker = !enabled;
    }
    pub fn key(&mut self, key: Key) -> Action {
        match key {
            Key::PageUp if !self.paste => return Action::Scroll(1),
            Key::PageDown if !self.paste => return Action::Scroll(-1),
            Key::Resize => return Action::Resize,
            _ => (),
        }
        if !self.paste {
            let choices = self.suggestions();
            match key {
                Key::Up if !choices.is_empty() => {
                    self.selected = self.selected.saturating_sub(1);
                    return Action::None;
                }
                Key::Down if !choices.is_empty() => {
                    self.selected = (self.selected + 1).min(choices.len() - 1);
                    return Action::None;
                }
                Key::Escape => {
                    self.dismiss_menu = true;
                    return Action::None;
                }
                Key::Palette if !self.disable_picker => {
                    self.text = "/".into();
                    self.cursor = 1;
                    self.selected = 0;
                    self.dismiss_menu = false;
                    return Action::None;
                }
                Key::Enter if !choices.is_empty() => {
                    let command =
                        crate::ui::COMMANDS[choices[self.selected.min(choices.len() - 1)]].0;
                    if self.text != command {
                        self.text = command.into();
                        self.cursor = self.text.len();
                        self.dismiss_menu = true;
                        return Action::None;
                    }
                }
                Key::Character(_) | Key::Backspace | Key::Delete | Key::Clear => {
                    self.selected = 0;
                    self.dismiss_menu = false;
                }
                _ => (),
            }
        }
        match key {
            Key::PasteStart => self.paste = true,
            Key::PasteEnd => self.paste = false,
            Key::Tab if !self.paste => return Action::Toggle,
            Key::Enter if !self.paste => {
                self.cursor = 0;
                self.selected = 0;
                self.dismiss_menu = false;
                return Action::Submit(std::mem::take(&mut self.text));
            }
            Key::Eof if self.text.is_empty() => return Action::Eof,
            Key::Character(ch) => self.insert(ch),
            Key::Tab => self.insert(' '),
            Key::Enter => self.insert('\n'),
            Key::Backspace if self.cursor > 0 => {
                let previous = self.text[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(n, _)| n)
                    .unwrap_or(0);
                self.text.drain(previous..self.cursor);
                self.cursor = previous;
            }
            Key::Delete | Key::Eof if self.cursor < self.text.len() => {
                let length = self.text[self.cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0);
                self.text.drain(self.cursor..self.cursor + length);
            }
            Key::Left if self.cursor > 0 => {
                self.cursor = self.text[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(n, _)| n)
                    .unwrap_or(0)
            }
            Key::Right if self.cursor < self.text.len() => {
                self.cursor += self.text[self.cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0)
            }
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.text.len(),
            Key::Clear => {
                self.text.clear();
                self.cursor = 0;
            }
            _ => (),
        }
        Action::None
    }
    fn insert(&mut self, ch: char) {
        if self.text.len() + ch.len_utf8() <= 100_000 {
            self.text.insert(self.cursor, ch);
            self.cursor += ch.len_utf8();
        }
    }
    pub fn render(&self) {
        if crate::ui::enabled() {
            crate::ui::edit(&self.text, self.cursor, self.suggestions(), self.selected);
            return;
        }
        use std::io::IsTerminal;
        if !io::stderr().is_terminal() {
            return;
        }
        eprint!("\rinput> {}", self.text.replace('\n', " ↵ "));
        let remaining = self.text[self.cursor..].chars().count();
        if remaining > 0 {
            eprint!("\x1b[{remaining}D");
        }
    }
    fn suggestions(&self) -> Vec<usize> {
        if self.dismiss_menu || self.paste || self.disable_picker {
            Vec::new()
        } else {
            crate::ui::suggestions(&self.text)
        }
    }
}

pub fn size() -> (usize, usize) {
    #[cfg(windows)]
    // SAFETY: reads console dimensions into initialized storage using a borrowed handle.
    #[expect(unsafe_code, reason = "native terminal viewport dimensions")]
    unsafe {
        use windows_sys::Win32::System::Console::*;
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(GetStdHandle(STD_ERROR_HANDLE), &mut info) != 0 {
            return (
                usize::try_from(info.srWindow.Right - info.srWindow.Left + 1).unwrap_or(80),
                usize::try_from(info.srWindow.Bottom - info.srWindow.Top + 1).unwrap_or(24),
            );
        }
    }
    #[cfg(unix)]
    // SAFETY: TIOCGWINSZ writes into an initialized winsize for stderr.
    #[expect(unsafe_code, reason = "native terminal viewport dimensions")]
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        if libc::ioctl(2, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0 {
            return (usize::from(size.ws_col), usize::from(size.ws_row).max(1));
        }
    }
    (80, 24)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcript_navigation_never_submits_or_changes_a_prompt() {
        let mut editor = Editor::default();
        editor.key(Key::Character('x'));
        assert_eq!(editor.key(Key::PageUp), Action::Scroll(1));
        assert_eq!(editor.key(Key::PageDown), Action::Scroll(-1));
        assert_eq!(editor.key(Key::Resize), Action::Resize);
        assert_eq!(editor.key(Key::Enter), Action::Submit("x".into()));
    }
    #[test]
    fn picker_inserts_without_submitting_and_escape_keeps_the_prompt() {
        let mut editor = Editor::default();
        editor.key(Key::Palette);
        editor.key(Key::Down);
        assert_eq!(editor.key(Key::Enter), Action::None);
        assert_eq!(editor.text, "/implement");
        assert_eq!(editor.key(Key::Enter), Action::Submit("/implement".into()));
        editor.key(Key::Character('/'));
        editor.key(Key::Escape);
        assert!(editor.suggestions().is_empty());
        assert_eq!(editor.key(Key::Enter), Action::Submit("/".into()));
    }
    #[test]
    fn pasted_commands_do_not_open_or_execute_the_picker() {
        let mut editor = Editor::default();
        editor.key(Key::PasteStart);
        editor.key(Key::Character('/'));
        editor.key(Key::Character('s'));
        editor.key(Key::Enter);
        assert!(editor.suggestions().is_empty());
        editor.key(Key::PasteEnd);
        assert_eq!(editor.key(Key::Enter), Action::Submit("/s\n".into()));
    }
    #[test]
    fn plain_terminal_never_intercepts_commands_with_an_invisible_picker() {
        let mut editor = Editor::default();
        editor.set_palette_enabled(false);
        editor.key(Key::Character('/'));
        editor.key(Key::Character('s'));
        assert!(editor.suggestions().is_empty());
        assert_eq!(editor.key(Key::Enter), Action::Submit("/s".into()));
    }
    #[test]
    fn tab_is_immediate_and_preserves_unsent_unicode_text() {
        let mut editor = Editor::default();
        editor.key(Key::Character('é'));
        editor.key(Key::Character('x'));
        editor.key(Key::Left);
        assert_eq!(editor.key(Key::Tab), Action::Toggle);
        assert_eq!(editor.key(Key::Tab), Action::Toggle);
        editor.key(Key::Character('a'));
        assert_eq!(editor.key(Key::Enter), Action::Submit("éax".into()));
    }
    #[test]
    fn pasted_tabs_do_not_switch_models_or_submit_partial_requests() {
        let mut editor = Editor::default();
        editor.key(Key::PasteStart);
        assert_eq!(editor.key(Key::Tab), Action::None);
        assert_eq!(editor.key(Key::Enter), Action::None);
        editor.key(Key::Character('x'));
        editor.key(Key::PasteEnd);
        assert_eq!(editor.key(Key::Enter), Action::Submit(" \nx".into()));
    }
    #[test]
    fn navigation_deletion_and_utf8_input_bounds_are_safe() {
        let mut editor = Editor::default();
        editor.key(Key::Character('😀'));
        editor.key(Key::Character('é'));
        editor.key(Key::Home);
        editor.key(Key::Delete);
        editor.key(Key::End);
        editor.key(Key::Backspace);
        assert_eq!(editor.key(Key::Eof), Action::Eof);
        for _ in 0..30_000 {
            editor.key(Key::Character('😀'));
        }
        let Action::Submit(text) = editor.key(Key::Enter) else {
            panic!("Expected bounded submission")
        };
        assert_eq!(text.len(), 100_000);
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }
}
