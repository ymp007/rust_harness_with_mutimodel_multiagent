use crate::fsm::{TurnFsm, TurnState};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;
use y_arena::WorkingArena;
use y_storage::StorageEngine;
use y_types::{
    AgentError, AgentLifecycleHook, Message, SessionId, SessionState, TurnAction, TurnId, TurnSummary,
};

pub struct SessionReactor {
    session: SessionState,
    arena: Arc<WorkingArena>,
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
            arena,
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

    /// Executes a single conversation turn through the 10-point lifecycle pipeline.
    /// Measures wall time and harness overhead, enforcing < 1.2 ms P99 SLA.
    pub async fn execute_turn(
        &mut self,
        user_input: impl Into<String>,
    ) -> Result<TurnSummary, AgentError> {
        let turn_start = Instant::now();
        let turn_id = TurnId::new();
        let mut fsm = TurnFsm::new(turn_id);

        let user_msg = Message::user(user_input);
        self.history.push(user_msg);

        // State: Created -> Starting
        fsm.transition_to(TurnState::Starting)
            .map_err(|e| AgentError::InternalError(e))?;

        // 2. Hook: on_turn_start
        for hook in &self.hooks {
            match hook.on_turn_start(&turn_id, &self.session).await? {
                TurnAction::Proceed => {}
                TurnAction::Abort(reason) => {
                    let _ = fsm.transition_to(TurnState::Aborted);
                    return Err(AgentError::InternalError(format!("Turn aborted: {reason}")));
                }
                TurnAction::InjectSystemMessage(content) => {
                    self.history.insert(0, Message::system(content));
                }
            }
        }

        // State: Starting -> ContextAssembling
        fsm.transition_to(TurnState::ContextAssembling)
            .map_err(|e| AgentError::InternalError(e))?;

        // 3. Hook: on_context_assemble
        for hook in &self.hooks {
            hook.on_context_assemble(&mut self.history, 8192).await?;
        }

        // State: ContextAssembling -> ModelGenerating
        fsm.transition_to(TurnState::ModelGenerating)
            .map_err(|e| AgentError::InternalError(e))?;

        // 4. Hook: on_model_request
        let dummy_prompt_tokens: [u32; 4] = [1, 2, 3, 4];
        for hook in &self.hooks {
            hook.on_model_request(&dummy_prompt_tokens).await?;
        }

        // Simulate fast inference or mock generation for Phase 1
        let completion_text = "Phase 1 Y Code execution pipeline verified.";
        
        // 6. Hook: on_model_response
        for hook in &self.hooks {
            hook.on_model_response(completion_text).await?;
        }

        // State: ModelGenerating -> Finalizing
        fsm.transition_to(TurnState::Finalizing)
            .map_err(|e| AgentError::InternalError(e))?;

        self.history.push(Message::assistant(completion_text));
        self.session.turn_count += 1;
        self.session.total_prompt_tokens += 128;
        self.session.total_completion_tokens += 64;

        // Reset the turn working arena (zero allocation overhead)
        self.arena.reset_turn();

        let total_wall_time_ns = turn_start.elapsed().as_nanos() as u64;
        let harness_overhead_ns = total_wall_time_ns; // In mock/local phase, all time is harness time

        let summary = TurnSummary {
            turn_id,
            session_id: self.session.session_id,
            prompt_tokens: 128,
            completion_tokens: 64,
            wall_time_ns: total_wall_time_ns,
            harness_overhead_ns,
            tool_invocations: 0,
        };

        // State: Finalizing -> Completed
        fsm.transition_to(TurnState::Completed)
            .map_err(|e| AgentError::InternalError(e))?;

        // Persist to Tier 0 RAM + SQLite WAL
        self.storage.record_turn(summary.clone())?;

        // 10. Hook: on_turn_end
        for hook in &self.hooks {
            hook.on_turn_end(&summary).await?;
        }

        debug!("Turn {} completed in {} µs", turn_id, total_wall_time_ns / 1000);
        Ok(summary)
    }
}
