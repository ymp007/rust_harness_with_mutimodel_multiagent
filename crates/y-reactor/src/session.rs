use crate::fsm::{TurnFsm, TurnState};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;
use y_arena::WorkingArena;
use y_storage::StorageEngine;
use y_types::{
    AgentError, AgentLifecycleHook, Message, Role, SessionId, SessionState, TurnAction, TurnId,
    TurnSummary,
};

/// Hard ceiling on the assembled context for one turn.
const CONTEXT_TOKEN_BUDGET: usize = 8_192;
/// Characters per token used by the estimator below.
const CHARS_PER_TOKEN: usize = 4;
/// System messages retained ahead of the user turn when packing.
const MIN_SYSTEM_MESSAGES: usize = 8;

pub struct SessionReactor {
    session: SessionState,
    _arena: Arc<WorkingArena>,
    storage: Arc<StorageEngine>,
    hooks: Vec<Arc<dyn AgentLifecycleHook>>,
    history: Vec<Message>,
}

impl SessionReactor {
    pub fn new(
        session_id: SessionId,
        arena: Arc<WorkingArena>,
        storage: Arc<StorageEngine>,
    ) -> Self {
        Self {
            session: SessionState::new(session_id),
            _arena: arena,
            storage,
            hooks: Vec::new(),
            history: Vec::new(),
        }
    }

    pub fn add_hook(&mut self, hook: Arc<dyn AgentLifecycleHook>) {
        self.hooks.push(hook);
    }

    pub fn session(&self) -> &SessionState {
        &self.session
    }

    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Approximate token count for budgeting. Deliberately cheap and stable:
    /// four characters per token, with a floor of one per message.
    pub fn estimate_tokens(messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| m.content.len().div_ceil(CHARS_PER_TOKEN).max(1))
            .sum()
    }

    /// Drops the oldest non-system messages until the context fits `budget`.
    /// The newest turns are always retained.
    fn pack_to_budget(messages: &mut Vec<Message>, budget: usize) -> Result<(), AgentError> {
        if Self::estimate_tokens(messages) <= budget {
            return Ok(());
        }
        let original = std::mem::take(messages);
        let mut retained: Vec<Message> = Vec::with_capacity(original.len());
        let mut system_kept = 0usize;
        for message in original.into_iter().rev() {
            let is_system = message.role == Role::System;
            let forced = is_system && system_kept < MIN_SYSTEM_MESSAGES;
            let mut trial = Vec::with_capacity(retained.len() + 1);
            trial.push(message);
            trial.extend(retained.iter().cloned());
            if forced || Self::estimate_tokens(&trial) <= budget {
                system_kept += usize::from(is_system);
                retained = trial;
            } else {
                break;
            }
        }
        retained.reverse();
        if retained.is_empty() {
            return Err(AgentError::ContextLimitExceeded(budget + 1, budget));
        }
        *messages = retained;
        Ok(())
    }

    /// Executes a single conversation turn through the lifecycle pipeline.
    ///
    /// The turn is assembled against a working copy of the history and session
    /// state is committed only after the summary persists, so a storage failure
    /// cannot leave a completed turn whose counters were advanced, and
    /// abandoning the future mid-flight leaves `self` untouched.
    ///
    /// `harness_overhead_ns` is wall time minus time attributed to model
    /// inference, so the two are not conflated.
    pub async fn execute_turn(
        &mut self,
        user_input: impl Into<String>,
    ) -> Result<TurnSummary, AgentError> {
        let turn_start = Instant::now();
        let turn_id = TurnId::new();
        let mut fsm = TurnFsm::new(turn_id);

        let result = self
            .run_turn(&mut fsm, turn_id, user_input.into(), turn_start)
            .await;

        if let Err(error) = &result {
            // Record a terminal state and notify hooks; a hook that backs off
            // after failure never runs otherwise.
            let _ = fsm.transition_to(TurnState::Failed);
            for hook in &self.hooks {
                if let Err(hook_error) = hook.on_error(error).await {
                    debug!("on_error hook failed: {hook_error}");
                }
            }
            debug!("Turn {turn_id} failed: {error}");
        }
        result
    }

    async fn run_turn(
        &mut self,
        fsm: &mut TurnFsm,
        turn_id: TurnId,
        user_input: String,
        turn_start: Instant,
    ) -> Result<TurnSummary, AgentError> {
        let fail = |e: String| AgentError::InternalError(e);

        fsm.transition_to(TurnState::Starting).map_err(fail)?;

        // Working copy: nothing below mutates `self` until the commit point.
        let mut history = self.history.clone();

        // Collect injections first so hook registration order is preserved.
        let mut injected: Vec<Message> = Vec::new();
        for hook in &self.hooks {
            match hook.on_turn_start(&turn_id, &self.session).await? {
                TurnAction::Proceed => {}
                TurnAction::Abort(reason) => {
                    let _ = fsm.transition_to(TurnState::Aborted);
                    return Err(AgentError::InternalError(format!("Turn aborted: {reason}")));
                }
                TurnAction::InjectSystemMessage(content) => {
                    injected.push(Message::system(content));
                }
            }
        }
        if !injected.is_empty() {
            history.splice(0..0, injected);
        }
        history.push(Message::user(user_input));

        fsm.transition_to(TurnState::ContextAssembling)
            .map_err(fail)?;
        Self::pack_to_budget(&mut history, CONTEXT_TOKEN_BUDGET)?;
        let prompt_tokens = u32::try_from(Self::estimate_tokens(&history)).unwrap_or(u32::MAX);

        for hook in &self.hooks {
            hook.on_context_assemble(&mut history, CONTEXT_TOKEN_BUDGET)
                .await?;
        }

        fsm.transition_to(TurnState::ModelGenerating)
            .map_err(fail)?;

        // Local completion path: no gateway call is bound here, so the measured
        // interval is empty and the whole turn counts as harness time.
        let prompt_token_ids: [u32; 4] = [1, 2, 3, 4];
        for hook in &self.hooks {
            hook.on_model_request(&prompt_token_ids).await?;
        }

        let model_start = Instant::now();
        let completion_text = "Phase 1 Y Code execution pipeline verified.";
        let model_ns = model_start.elapsed().as_nanos() as u64;

        for hook in &self.hooks {
            hook.on_model_response(completion_text).await?;
        }

        fsm.transition_to(TurnState::Finalizing).map_err(fail)?;

        let completion_tokens = u32::try_from(Self::estimate_tokens(&[Message::assistant(
            completion_text,
        )]))
        .unwrap_or(u32::MAX);
        history.push(Message::assistant(completion_text));

        let total_wall_time_ns = turn_start.elapsed().as_nanos() as u64;
        let summary = TurnSummary {
            turn_id,
            session_id: self.session.session_id,
            prompt_tokens,
            completion_tokens,
            wall_time_ns: total_wall_time_ns,
            // Harness cost excludes inference time so the metric stays
            // meaningful once a real gateway call replaces the local echo.
            harness_overhead_ns: total_wall_time_ns.saturating_sub(model_ns),
            tool_invocations: 0,
        };

        // Persist before committing any in-memory state.
        self.storage.record_turn(summary.clone())?;
        fsm.transition_to(TurnState::Completed).map_err(fail)?;

        // Commit: the turn is durable, so state advances atomically here.
        self.history = history;
        self.session.turn_count += 1;
        self.session.total_prompt_tokens += u64::from(prompt_tokens);
        self.session.total_completion_tokens += u64::from(completion_tokens);

        for hook in &self.hooks {
            if let Err(hook_error) = hook.on_turn_end(&summary).await {
                // The turn is committed and durable; a failing observer must
                // not undo it.
                debug!("on_turn_end hook failed: {hook_error}");
            }
        }

        debug!(
            "Turn {turn_id} completed in {} µs",
            total_wall_time_ns / 1000
        );
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn engine() -> (Arc<WorkingArena>, Arc<StorageEngine>, std::path::PathBuf) {
        let db_path = std::env::temp_dir().join(format!("reactor-{}.db", TurnId::new()));
        let arena = Arc::new(WorkingArena::with_defaults());
        let storage = Arc::new(StorageEngine::with_defaults(&db_path).unwrap());
        (arena, storage, db_path)
    }

    #[tokio::test]
    async fn token_counts_are_derived_from_history() {
        let (arena, storage, db_path) = engine();
        let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());
        let summary = reactor.execute_turn("hello there").await.unwrap();
        assert!(summary.prompt_tokens > 0);
        assert!(summary.completion_tokens > 0);
        // Prompt size covers everything sent to the model, which excludes the
        // assistant reply appended at the end of the turn.
        let sent: Vec<Message> = reactor
            .history()
            .iter()
            .filter(|m| m.role != Role::Assistant)
            .cloned()
            .collect();
        assert_eq!(
            summary.prompt_tokens as usize,
            SessionReactor::estimate_tokens(&sent)
        );
        assert_eq!(reactor.session().turn_count, 1);
        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn harness_overhead_excludes_model_time() {
        let (arena, storage, db_path) = engine();
        let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());
        let summary = reactor.execute_turn("hello").await.unwrap();
        assert!(summary.harness_overhead_ns <= summary.wall_time_ns);
        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn history_is_trimmed_to_the_context_budget() {
        let (arena, storage, db_path) = engine();
        let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());
        for i in 0..80 {
            reactor
                .execute_turn(format!("a reasonably long message number {i}"))
                .await
                .unwrap();
        }
        assert!(
            SessionReactor::estimate_tokens(reactor.history()) <= CONTEXT_TOKEN_BUDGET,
            "context exceeded the budget"
        );
        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn failed_turn_leaves_no_orphaned_message() {
        // Fails the first turn, then allows later turns through so recovery can be
        // observed on the same reactor.
        struct FlakyHook {
            fired: AtomicBool,
            saw_error: AtomicBool,
        }
        impl AgentLifecycleHook for FlakyHook {
            fn on_turn_start<'a>(
                &'a self,
                _: &'a TurnId,
                _: &'a SessionState,
            ) -> y_types::hook::BoxFuture<'a, Result<TurnAction, AgentError>> {
                if self.fired.swap(true, Ordering::AcqRel) {
                    Box::pin(async { Ok(TurnAction::Proceed) })
                } else {
                    Box::pin(async { Err(AgentError::ProviderError("quota exhausted".into())) })
                }
            }
            fn on_error<'a>(
                &'a self,
                _: &'a AgentError,
            ) -> y_types::hook::BoxFuture<'a, Result<(), AgentError>> {
                self.saw_error.store(true, Ordering::Release);
                Box::pin(async { Ok(()) })
            }
        }

        let (arena, storage, db_path) = engine();
        let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());
        let hook = Arc::new(FlakyHook {
            fired: AtomicBool::new(false),
            saw_error: AtomicBool::new(false),
        });
        reactor.add_hook(hook.clone());

        assert!(reactor.execute_turn("this must not stick").await.is_err());
        assert!(
            hook.saw_error.load(Ordering::Acquire),
            "on_error must run when a turn fails"
        );
        assert!(reactor.history().is_empty(), "orphaned message retained");
        assert_eq!(reactor.session().turn_count, 0);

        // The next turn must still succeed on the same reactor.
        let summary = reactor.execute_turn("retry").await.unwrap();
        assert_eq!(reactor.session().turn_count, 1);
        assert!(summary.completion_tokens > 0);

        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn injected_system_messages_keep_registration_order() {
        struct Injector(&'static str);
        impl AgentLifecycleHook for Injector {
            fn on_turn_start<'a>(
                &'a self,
                _: &'a TurnId,
                _: &'a SessionState,
            ) -> y_types::hook::BoxFuture<'a, Result<TurnAction, AgentError>> {
                Box::pin(async { Ok(TurnAction::InjectSystemMessage(self.0.to_string())) })
            }
        }

        let (arena, storage, db_path) = engine();
        let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());
        reactor.add_hook(Arc::new(Injector("first")));
        reactor.add_hook(Arc::new(Injector("second")));

        reactor.execute_turn("go").await.unwrap();
        let leading: Vec<&str> = reactor
            .history()
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.content.as_str())
            .collect();
        assert_eq!(leading, vec!["first", "second"]);
        storage.shutdown();
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn oversized_single_message_is_rejected() {
        let mut messages = vec![Message::user("x".repeat(CONTEXT_TOKEN_BUDGET * 8))];
        let err = SessionReactor::pack_to_budget(&mut messages, CONTEXT_TOKEN_BUDGET).unwrap_err();
        assert!(matches!(err, AgentError::ContextLimitExceeded(_, _)));
    }
}
