use y_types::id::TurnId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Created,
    Starting,
    ContextAssembling,
    ModelGenerating,
    ToolExecuting,
    Finalizing,
    Completed,
    Aborted,
    Failed,
}

#[derive(Debug)]
pub struct TurnFsm {
    turn_id: TurnId,
    state: TurnState,
    transition_count: usize,
}

impl TurnFsm {
    pub fn new(turn_id: TurnId) -> Self {
        Self {
            turn_id,
            state: TurnState::Created,
            transition_count: 0,
        }
    }

    pub fn turn_id(&self) -> TurnId {
        self.turn_id
    }

    pub fn state(&self) -> TurnState {
        self.state
    }

    pub fn transition_to(&mut self, next: TurnState) -> Result<(), String> {
        let valid = match (self.state, next) {
            (TurnState::Created, TurnState::Starting) => true,
            (TurnState::Starting, TurnState::ContextAssembling) => true,
            (TurnState::Starting, TurnState::Aborted) => true,
            (TurnState::ContextAssembling, TurnState::ModelGenerating) => true,
            (TurnState::ModelGenerating, TurnState::ToolExecuting) => true,
            (TurnState::ModelGenerating, TurnState::Finalizing) => true,
            (TurnState::ToolExecuting, TurnState::ModelGenerating) => true,
            (TurnState::ToolExecuting, TurnState::Finalizing) => true,
            (TurnState::Finalizing, TurnState::Completed) => true,
            (_, TurnState::Failed) => true,
            (_, TurnState::Aborted) => true,
            _ => false,
        };

        if valid {
            self.state = next;
            self.transition_count += 1;
            Ok(())
        } else {
            Err(format!(
                "Invalid Turn FSM transition: {:?} -> {:?}",
                self.state, next
            ))
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            TurnState::Completed | TurnState::Aborted | TurnState::Failed
        )
    }
}
