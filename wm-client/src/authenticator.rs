use std::sync::Arc;

use crate::agent::Agent;
use crate::configuration::Configuration;

pub struct AgentAuthenticator {
    _configuration: Arc<Configuration>,
}

impl AgentAuthenticator {
    pub fn new(configuration: Arc<Configuration>) -> Self {
        Self {
            _configuration: configuration,
        }
    }

    pub async fn run(&self) {
        let agent = Agent::new(self._configuration.clone());
        agent.authenticate().await;
    }
}
