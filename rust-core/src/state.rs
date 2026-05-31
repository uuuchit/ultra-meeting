//! Recording state machine.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

use crate::error::RecordingError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingState {
    Idle,
    Preparing,
    Recording,
    Stopping,
    Processing,
    Error(String),
    Failed(String),
}

impl RecordingState {
    pub fn display_name(&self) -> &'static str {
        match self {
            RecordingState::Idle => "idle",
            RecordingState::Preparing => "preparing",
            RecordingState::Recording => "recording",
            RecordingState::Stopping => "stopping",
            RecordingState::Processing => "processing",
            RecordingState::Error(_) => "error",
            RecordingState::Failed(_) => "failed",
        }
    }
}

pub struct StateMachine {
    state: RecordingState,
    storage_path: PathBuf,
    state_path: PathBuf,
}

impl StateMachine {
    pub fn new() -> Result<Self, RecordingError> {
        let state_dir = dirs::home_dir()
            .ok_or_else(|| RecordingError::Other("no home dir".into()))?
            .join(".ultra-meeting");
        std::fs::create_dir_all(&state_dir)?;
        let state_path = state_dir.join("state.json");

        let state = if state_path.exists() {
            let data = std::fs::read_to_string(&state_path)?;
            serde_json::from_str(&data).unwrap_or(RecordingState::Idle)
        } else {
            RecordingState::Idle
        };

        let storage_path = dirs::document_dir()
            .ok_or_else(|| RecordingError::Other("no documents dir".into()))?
            .join("UltraMeeting/recordings");
        std::fs::create_dir_all(&storage_path)?;

        Ok(Self {
            state,
            storage_path,
            state_path,
        })
    }

    pub fn state(&self) -> &RecordingState {
        &self.state
    }

    pub fn persist(&mut self) -> Result<(), RecordingError> {
        let data = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&self.state_path, data)?;
        Ok(())
    }

    pub fn transition_to(&mut self, new: RecordingState) -> Result<(), RecordingError> {
        let old = std::mem::replace(&mut self.state, new.clone());
        info!("State: {} -> {}", old.display_name(), new.display_name());
        self.persist()?;
        Ok(())
    }

    pub fn start_preparing(&mut self) -> Result<(), RecordingError> {
        match &self.state {
            RecordingState::Idle => self.transition_to(RecordingState::Preparing),
            _ => Err(RecordingError::InvalidState(format!(
                "cannot start from {:?}",
                self.state
            ))),
        }
    }

    pub fn start_recording(&mut self) -> Result<(), RecordingError> {
        match &self.state {
            RecordingState::Preparing => self.transition_to(RecordingState::Recording),
            _ => Err(RecordingError::InvalidState(format!(
                "cannot record from {:?}",
                self.state
            ))),
        }
    }

    pub fn stop_recording(&mut self) -> Result<(), RecordingError> {
        match &self.state {
            RecordingState::Recording => self.transition_to(RecordingState::Stopping),
            _ => Err(RecordingError::InvalidState(format!(
                "cannot stop from {:?}",
                self.state
            ))),
        }
    }

    pub fn start_processing(&mut self) -> Result<(), RecordingError> {
        match &self.state {
            RecordingState::Stopping => self.transition_to(RecordingState::Processing),
            _ => Err(RecordingError::InvalidState(format!(
                "cannot process from {:?}",
                self.state
            ))),
        }
    }

    pub fn finish(&mut self) -> Result<(), RecordingError> {
        match &self.state {
            RecordingState::Processing => self.transition_to(RecordingState::Idle),
            RecordingState::Error(_) => self.transition_to(RecordingState::Idle),
            _ => Err(RecordingError::InvalidState(format!(
                "cannot finish from {:?}",
                self.state
            ))),
        }
    }

    pub fn set_error(&mut self, msg: String) -> Result<(), RecordingError> {
        self.transition_to(RecordingState::Error(msg))
    }

    pub fn set_failed(&mut self, msg: String) -> Result<(), RecordingError> {
        self.transition_to(RecordingState::Failed(msg))
    }

    /// Force reset to Idle (for crash recovery when state was Recording/Stopping/Processing).
    pub fn reset_interrupted(&mut self) -> Result<(), RecordingError> {
        match &self.state {
            RecordingState::Idle | RecordingState::Preparing => Ok(()),
            _ => self.transition_to(RecordingState::Idle),
        }
    }

    pub fn storage_path(&self) -> &PathBuf {
        &self.storage_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine_at(state: RecordingState) -> (StateMachine, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("ultra-meeting-state-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let machine = StateMachine {
            state,
            storage_path: root.join("recordings"),
            state_path: root.join("state.json"),
        };
        (machine, root)
    }

    #[test]
    fn state_transitions() {
        let _ = tracing_subscriber::fmt().try_init();
        let (mut machine, root) = machine_at(RecordingState::Idle);

        machine.start_preparing().unwrap();
        assert_eq!(machine.state(), &RecordingState::Preparing);
        machine.start_recording().unwrap();
        assert_eq!(machine.state(), &RecordingState::Recording);
        machine.stop_recording().unwrap();
        assert_eq!(machine.state(), &RecordingState::Stopping);
        machine.start_processing().unwrap();
        assert_eq!(machine.state(), &RecordingState::Processing);
        machine.finish().unwrap();
        assert_eq!(machine.state(), &RecordingState::Idle);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_state_transitions() {
        let (mut machine, root) = machine_at(RecordingState::Idle);

        assert!(matches!(
            machine.stop_recording().unwrap_err(),
            RecordingError::InvalidState(_)
        ));
        assert!(matches!(
            machine.start_processing().unwrap_err(),
            RecordingError::InvalidState(_)
        ));
        assert!(matches!(
            machine.finish().unwrap_err(),
            RecordingError::InvalidState(_)
        ));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_interrupted_recovers_non_idle_runtime_states() {
        for state in [
            RecordingState::Recording,
            RecordingState::Stopping,
            RecordingState::Processing,
            RecordingState::Error("boom".into()),
            RecordingState::Failed("boom".into()),
        ] {
            let (mut machine, root) = machine_at(state);
            machine.reset_interrupted().unwrap();
            assert_eq!(machine.state(), &RecordingState::Idle);
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn persists_state_to_disk() {
        let (mut machine, root) = machine_at(RecordingState::Idle);

        machine.start_preparing().unwrap();
        let data = std::fs::read_to_string(root.join("state.json")).unwrap();
        let persisted: RecordingState = serde_json::from_str(&data).unwrap();

        assert_eq!(persisted, RecordingState::Preparing);
        let _ = std::fs::remove_dir_all(root);
    }
}
