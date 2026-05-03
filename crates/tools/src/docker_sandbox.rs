use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions, RemoveContainerOptions, LogOutput};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures::StreamExt;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DockerSandbox {
    docker: Docker,
    image: String,
    timeout: std::time::Duration,
    container_id: Arc<Mutex<Option<String>>>,
}

impl DockerSandbox {
    pub fn new(image: &str) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            image: image.to_string(),
            timeout: std::time::Duration::from_secs(30),
            container_id: Arc::new(Mutex::new(None)),
        })
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn ensure_container(&self) -> Result<String> {
        let mut id_lock = self.container_id.lock().await;
        
        if let Some(id) = id_lock.as_ref() {
            // Check if container is still alive
            if let Ok(_) = self.docker.inspect_container(id, None).await {
                return Ok(id.clone());
            }
        }

        log::info!("Creating a new persistent sandbox container for image: {}", self.image);
        let name = format!("pharmakon-persistent-sandbox-{}", uuid::Uuid::new_v4());
        
        let config = Config {
            image: Some(self.image.clone()),
            entrypoint: Some(vec!["sh".to_string(), "-c".to_string(), "sleep infinity".to_string()]),
            ..Default::default()
        };

        self.docker.create_container(
            Some(CreateContainerOptions { name: name.clone(), platform: None }),
            config,
        ).await?;

        self.docker.start_container(&name, None::<StartContainerOptions<String>>).await?;
        
        *id_lock = Some(name.clone());
        Ok(name)
    }

    pub async fn run_command(&self, command: &str) -> Result<(String, String)> {
        let container_name = self.ensure_container().await?;

        let exec_config = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), command.to_string()]),
            ..Default::default()
        };

        let exec = self.docker.create_exec(&container_name, exec_config).await?;
        let mut stdout = String::new();
        let mut stderr = String::new();

        let run_future = async {
            if let StartExecResults::Attached { mut output, .. } = self.docker.start_exec(&exec.id, None).await? {
                while let Some(msg) = output.next().await {
                    match msg? {
                        LogOutput::StdOut { message } => stdout.push_str(&String::from_utf8_lossy(&message)),
                        LogOutput::StdErr { message } => stderr.push_str(&String::from_utf8_lossy(&message)),
                        _ => {}
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        };

        let run_result = tokio::time::timeout(self.timeout, run_future).await;
        
        if let Err(_) = run_result {
            stderr.push_str(&format!("\n[Error: Command timed out after {:?}]", self.timeout));
        } else if let Ok(Err(e)) = run_result {
            return Err(e);
        }

        Ok((stdout, stderr))
    }

    pub async fn cleanup(&self) -> Result<()> {
        let mut id_lock = self.container_id.lock().await;
        if let Some(id) = id_lock.take() {
            log::info!("Cleaning up sandbox container: {}", id);
            let _ = self.docker.remove_container(
                &id,
                Some(RemoveContainerOptions { force: true, ..Default::default() }),
            ).await;
        }
        Ok(())
    }
}

