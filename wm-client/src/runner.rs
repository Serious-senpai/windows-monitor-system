use std::error::Error;
use std::sync::Arc;

use log::{debug, info, warn};
use tokio::sync::SetOnce;
use tokio::task::JoinHandle;
use tokio::{signal, task};
use windows_services::{Command, Service};

use crate::agent::Agent;
use crate::configuration::Configuration;

pub struct AgentRunner {
    _configuration: Arc<Configuration>,
    _service_handle: Option<JoinHandle<Result<(), &'static str>>>,
    _service_stopped: Arc<SetOnce<()>>,
}

impl AgentRunner {
    pub fn new<const SERVICE: bool>(configuration: Arc<Configuration>) -> Self {
        let stopped = Arc::new(SetOnce::new());
        let stopped_clone = stopped.clone();

        let handle = if SERVICE {
            Some(task::spawn_blocking(move || {
                Service::new().can_stop().run(|_, command| {
                    debug!("Received service command: {command:?}");

                    match command {
                        Command::Stop => {
                            info!("Stopping service");
                            let _ = stopped_clone.set(());
                        }
                        _ => {
                            warn!("Unsupported service command {command:?}")
                        }
                    }
                })
            }))
        } else {
            None
        };

        Self {
            _configuration: configuration,
            _service_handle: handle,
            _service_stopped: stopped,
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let password = Agent::read_password(&self._configuration).await;
        let agent = Arc::new(Agent::new(self._configuration.clone(), &password).await);

        let ptr = agent.clone();
        let mut agent_handle = tokio::spawn(async move {
            ptr.run().await;
        });

        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C signal");
            },
            _ = &mut agent_handle => (),
            _ = self._service_stopped.wait() => {
                info!("Service stopped");
            }
        };

        info!("Stopping agent");
        agent.stop().await;
        agent_handle.await?;

        if let Some(h) = &mut self._service_handle {
            h.await??;
        }

        Ok(())
    }
}
