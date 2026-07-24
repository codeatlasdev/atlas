#![allow(unused)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityState {
    Idle,
    Active,
    WaitingInput,
    Blocked,
    Exited(i32),
}

pub trait ActivityDetector: Send + Sync {
    fn detect(&self, output: &[u8]) -> Option<ActivityState>;
}
