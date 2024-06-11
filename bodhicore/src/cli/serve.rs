use super::{CliError, Command};
use crate::{
  db::{DbPool, DbService, DbServiceFn, TimeService},
  error::Common,
  server::{build_routes, build_server_handle, shutdown_signal, ServerHandle, ShutdownCallback},
  service::AppServiceFn,
  BodhiError, SharedContextRw, SharedContextRwFn,
};
use axum::Router;
use std::sync::Arc;
use tokio::{runtime::Builder, sync::oneshot::Sender, task::JoinHandle};

#[derive(Debug, Clone, PartialEq)]
pub enum ServeCommand {
  ByParams { host: String, port: u16 },
}

impl TryFrom<Command> for ServeCommand {
  type Error = CliError;

  fn try_from(value: Command) -> Result<Self, Self::Error> {
    match value {
      Command::Serve { host, port } => Ok(ServeCommand::ByParams { host, port }),
      cmd => Err(CliError::ConvertCommand(
        cmd.to_string(),
        "serve".to_string(),
      )),
    }
  }
}

pub struct ShutdownContextCallback {
  ctx: Arc<dyn SharedContextRwFn>,
}

#[async_trait::async_trait]
impl ShutdownCallback for ShutdownContextCallback {
  async fn shutdown(&self) {
    if let Err(err) = self.ctx.try_stop().await {
      tracing::warn!(err = ?err, "error stopping llama context");
    }
  }
}

pub struct ServerShutdownHandle {
  join_handle: JoinHandle<Result<(), BodhiError>>,
  shutdown: Sender<()>,
}

impl ServerShutdownHandle {
  pub async fn shutdown_on_ctrlc(self) -> crate::error::Result<()> {
    shutdown_signal().await;
    self.shutdown().await?;
    Ok(())
  }

  pub async fn shutdown(self) -> crate::error::Result<()> {
    match self.shutdown.send(()) {
      Ok(()) => {}
      Err(err) => tracing::warn!(?err, "error sending shutdown signal on shutdown channel"),
    };
    (self.join_handle.await.map_err(Common::Join)?)?;
    Ok(())
  }
}

impl ServeCommand {
  pub fn execute(&self, service: Arc<dyn AppServiceFn>) -> crate::error::Result<()> {
    match self {
      ServeCommand::ByParams { host, port } => {
        self.execute_by_params(host, *port, service, None)?;
        Ok(())
      }
    }
  }

  pub async fn aexecute(
    &self,
    service: Arc<dyn AppServiceFn>,
    static_router: Option<Router>,
  ) -> crate::error::Result<ServerShutdownHandle> {
    match self {
      ServeCommand::ByParams { host, port } => {
        let handle = self
          .aexecute_by_params(host, *port, service, static_router)
          .await?;
        Ok(handle)
      }
    }
  }

  fn execute_by_params(
    &self,
    host: &str,
    port: u16,
    service: Arc<dyn AppServiceFn>,
    static_router: Option<Router>,
  ) -> crate::error::Result<()> {
    let runtime = Builder::new_multi_thread()
      .enable_all()
      .build()
      .map_err(Common::from)?;
    runtime.block_on(async move {
      let handle = self
        .aexecute_by_params(host, port, service, static_router)
        .await?;
      handle.shutdown_on_ctrlc().await?;
      Ok::<(), BodhiError>(())
    })?;
    Ok(())
  }

  async fn aexecute_by_params(
    &self,
    host: &str,
    port: u16,
    service: Arc<dyn AppServiceFn>,
    static_router: Option<Router>,
  ) -> crate::error::Result<ServerShutdownHandle> {
    let dbpath = service.env_service().db_path();
    let pool = DbPool::connect(&format!("sqlite:{}", dbpath.display())).await?;
    let db_service = DbService::new(pool, Arc::new(TimeService));
    db_service.migrate().await?;

    let ServerHandle {
      server,
      shutdown,
      ready_rx,
    } = build_server_handle(host, port);

    let ctx = SharedContextRw::new_shared_rw(None).await?;
    let ctx: Arc<dyn SharedContextRwFn> = Arc::new(ctx);
    let app = build_routes(ctx.clone(), service, Arc::new(db_service), static_router);

    let join_handle = tokio::spawn(async move {
      let callback = Box::new(ShutdownContextCallback { ctx });
      match server.start_new(app, Some(callback)).await {
        Ok(()) => Ok(()),
        Err(err) => {
          tracing::error!(err = ?err, "server encountered an error");
          Err(err)
        }
      }
    });
    match ready_rx.await {
      Ok(()) => {}
      Err(err) => tracing::warn!(?err, "ready channel closed before could receive signal"),
    }
    Ok(ServerShutdownHandle {
      join_handle,
      shutdown,
    })
  }
}

#[cfg(test)]
mod test {
  use super::{Command, ServeCommand};
  use rstest::rstest;

  #[rstest]
  fn test_serve_command_from_serve() -> anyhow::Result<()> {
    let cmd = Command::Serve {
      host: "localhost".to_string(),
      port: 1135,
    };
    let result = ServeCommand::try_from(cmd)?;
    let expected = ServeCommand::ByParams {
      host: "localhost".to_string(),
      port: 1135,
    };
    assert_eq!(expected, result);
    Ok(())
  }

  #[rstest]
  fn test_serve_command_convert_err() -> anyhow::Result<()> {
    let cmd = Command::List {
      remote: false,
      models: false,
    };
    let result = ServeCommand::try_from(cmd);
    assert!(result.is_err());
    assert_eq!(
      "Command 'list' cannot be converted into command 'serve'",
      result.unwrap_err().to_string()
    );
    Ok(())
  }
}
