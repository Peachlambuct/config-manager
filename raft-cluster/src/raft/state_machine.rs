use std::collections::HashMap;

pub struct ConfigStateMachine {
    pub config: HashMap<String, String>,
}

impl ConfigStateMachine {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
        }
    }
}
