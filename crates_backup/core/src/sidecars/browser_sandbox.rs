use anyhow::Result;
use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::service::HostConfig;
use futures::StreamExt;
use std::collections::HashMap;

pub struct BrowserSandbox {
    docker: Docker,
    container_name: String,
    image_name: String,
}

impl BrowserSandbox {
    pub fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            container_name: "pharmakon-browser-sandbox".to_string(),
            image_name: "browserless/chrome:latest".to_string(),
        })
    }

    pub async fn ensure_started(&self) -> Result<u16> {
        // 1. Pull image if not exists
        let mut pull_stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: self.image_name.clone(),
                ..Default::default()
            }),
            None,
            None,
        );

        while let Some(pull_result) = pull_stream.next().await {
            if let Err(e) = pull_result {
                log::warn!("Docker pull warning: {}", e);
            }
        }

        // 2. Check if container exists and is running
        let containers = self.docker.list_containers::<String>(None).await?;
        if let Some(c) = containers.iter().find(|c| {
            c.names
                .as_ref()
                .map(|n| n.iter().any(|name| name.contains(&self.container_name)))
                .unwrap_or(false)
        })
            && c.state.as_deref() == Some("running") {
                return Ok(3030); // Use new port
            }

        // 3. Create and start container
        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            "3000/tcp".to_string(), // Container internal port stays 3000
            Some(vec![bollard::service::PortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some("3030".to_string()), // Host port changed to 3030
            }]),
        );

        let config = Config {
            image: Some(self.image_name.clone()),
            host_config: Some(HostConfig {
                port_bindings: Some(port_bindings),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: self.container_name.clone(),
                    ..Default::default()
                }),
                config,
            )
            .await?;

        self.docker
            .start_container(&self.container_name, None::<StartContainerOptions<String>>)
            .await?;

        log::info!("Browser sandbox container started on port 3030");
        Ok(3030)
    }

    pub async fn stop(&self) -> Result<()> {
        let _ = self.docker.stop_container(&self.container_name, None).await;
        let _ = self
            .docker
            .remove_container(&self.container_name, None)
            .await;
        Ok(())
    }
}
