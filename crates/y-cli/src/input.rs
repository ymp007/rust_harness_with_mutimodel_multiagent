use crate::tools::Result;
use std::{
    io::{self, BufRead, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};

struct Broker {
    lines: AsyncMutex<mpsc::Receiver<String>>,
    pending: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
    approvals: AsyncMutex<()>,
    closed: Arc<AtomicBool>,
}
static BROKER: OnceLock<Broker> = OnceLock::new();
static MULTI_MODEL: AtomicBool = AtomicBool::new(true);
static PROFILE_CONTEXT: OnceLock<Mutex<Option<(crate::tools::Workspace, String)>>> =
    OnceLock::new();

pub fn configure_profile(ws: &crate::tools::Workspace, id: &str, on: bool) -> Result<()> {
    let mut context = PROFILE_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "Profile state poisoned")?;
    crate::models::save_mode(ws, id, on)?;
    *context = Some((ws.clone(), id.into()));
    MULTI_MODEL.store(on, Ordering::SeqCst);
    Ok(())
}
pub fn set_profile(on: bool) -> Result<()> {
    if let Some(context) = PROFILE_CONTEXT.get() {
        let context = context.lock().map_err(|_| "Profile state poisoned")?;
        if let Some((ws, id)) = &*context {
            crate::models::save_mode(ws, id, on)?;
        }
    }
    MULTI_MODEL.store(on, Ordering::SeqCst);
    Ok(())
}
pub fn profile() -> Option<bool> {
    active().then(|| MULTI_MODEL.load(Ordering::SeqCst))
}

/// One reader owns terminal stdin. Approval replies never become user steering.
pub fn start() {
    if BROKER.get().is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel(32);
    let pending: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(None));
    let closed = Arc::new(AtomicBool::new(false));
    BROKER
        .set(Broker {
            lines: AsyncMutex::new(rx),
            pending: pending.clone(),
            approvals: AsyncMutex::new(()),
            closed: closed.clone(),
        })
        .ok();
    tokio::task::spawn_blocking(move || {
        let raw = match crate::terminal::start() {
            Ok(guard) => Some(guard),
            Err(e) => {
                crate::ui::eprintln!("Native key input unavailable ({e}); use /multimodel to toggle and Enter to submit.");
                None
            }
        };
        if raw.is_some() {
            crate::ui::start(true);
            let mut reader = crate::terminal::Reader::new();
            let mut editor = crate::terminal::Editor::default();
            editor.set_palette_enabled(crate::ui::enabled());
            loop {
                let key = match reader.read() {
                    Ok(key) => key,
                    Err(e) => {
                        crate::ui::eprintln!("Terminal input failed: {e}");
                        break;
                    }
                };
                match editor.key(key) {
                    crate::terminal::Action::None => (),
                    crate::terminal::Action::Toggle => {
                        let on = !MULTI_MODEL.load(Ordering::SeqCst);
                        match set_profile(on) {
                            Ok(()) => {
                                if tx
                                    .blocking_send(format!(
                                        "/multimodel {}",
                                        if on { "on" } else { "off" }
                                    ))
                                    .is_err()
                                {
                                    break;
                                }
                                crate::ui::multi(on);
                            }
                            Err(e) => crate::ui::eprintln!("\r\nProfile toggle failed: {e}"),
                        }
                    }
                    crate::terminal::Action::Submit(line) => {
                        crate::ui::submit(&line);
                        if !route(line, &pending, &tx) {
                            break;
                        }
                    }
                    crate::terminal::Action::Eof => break,
                    crate::terminal::Action::Scroll(direction) => crate::ui::scroll(direction),
                    crate::terminal::Action::Resize => {
                        crate::ui::resize();
                        continue;
                    }
                }
                editor.render();
                let _ = io::stderr().flush();
            }
        } else {
            crate::ui::start(false);
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            loop {
                eprint!("ycode> ");
                let _ = io::stderr().flush();
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    _ => (),
                }
                if line.len() > 100_000 {
                    crate::ui::eprintln!("Input exceeds 100 KiB; ignored.");
                    continue;
                }
                if !route(line, &pending, &tx) {
                    break;
                }
            }
        }
        crate::ui::stop();
        drop(raw);
        closed.store(true, Ordering::SeqCst);
        if let Some(reply) = pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = reply.send(false);
        }
    });
}

fn route(
    line: String,
    pending: &Mutex<Option<oneshot::Sender<bool>>>,
    tx: &mpsc::Sender<String>,
) -> bool {
    let answer = match line.trim() {
        "/approve y" => Some(true),
        "/approve n" | "" => Some(false),
        _ => None,
    };
    if let Some(answer) = answer {
        if let Some(reply) = pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = reply.send(answer);
            return true;
        }
    }
    tx.blocking_send(line).is_ok()
}

pub fn active() -> bool {
    BROKER.get().is_some()
}

pub async fn line() -> Option<String> {
    BROKER.get()?.lines.lock().await.recv().await
}

pub async fn approve(kind: &str, detail: &str) -> Result<bool> {
    let broker = BROKER.get().ok_or("Terminal input broker is not running")?;
    let _guard = broker.approvals.lock().await;
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = broker
            .pending
            .lock()
            .map_err(|_| "Approval state poisoned")?;
        if broker.closed.load(Ordering::SeqCst) {
            return Ok(false);
        }
        *pending = Some(tx);
    }
    crate::ui::eprintln!("\n{}: {}\nApprove with /approve y or deny with /approve n. Other input steers the active task.", crate::tools::display(kind), crate::tools::display(detail));
    io::stderr().flush()?;
    Ok(rx.await.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_controls_never_answer_a_pending_approval() {
        let (tx, mut lines) = mpsc::channel(4);
        let (reply, mut answer) = oneshot::channel();
        let pending = Mutex::new(Some(reply));
        assert!(route("/multimodel off".into(), &pending, &tx));
        assert_eq!(lines.try_recv().unwrap(), "/multimodel off");
        assert!(answer.try_recv().is_err());
        assert!(route("/approve n".into(), &pending, &tx));
        assert!(!answer.try_recv().unwrap());
        assert!(lines.try_recv().is_err());
    }
}
