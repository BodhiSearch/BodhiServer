use crate::{
  build_routes, build_server_handle,
  cli::{Cli, Command},
  server::ServerHandle,
  shutdown_signal, List, Pull, Run, Serve, SharedContextRw, SharedContextRwExts,
};
use anyhow::Context;
use clap::Parser;
use futures_util::{future::BoxFuture, FutureExt};
use tokio::runtime::Builder;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn main_internal() -> anyhow::Result<()> {
  setup_logs()?;
  let cli = Cli::parse();
  match cli.command {
    Command::Serve { host, port, model } => {
      main_async(Serve { host, port, model })?;
    }
    Command::Pull {
      id,
      repo,
      file,
      force,
    } => {
      let pull_param = Pull::new(id, repo, file, force);
      pull_param.execute()?;
    }
    Command::List { remote } => {
      List::new(remote).execute()?;
    }
    Command::Run { id, repo, file } => {
      Run::new(id, repo, file).execute()?;
    }
  }
  Ok(())
}

fn setup_logs() -> anyhow::Result<()> {
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
  let filter = filter.add_directive("hf_hub=error".parse().unwrap());
  tracing_subscriber::registry()
    .with(filter)
    .with(fmt::layer())
    .init();
  Ok(())
}

fn main_async(serve: Serve) -> anyhow::Result<()> {
  let runtime = Builder::new_multi_thread().enable_all().build();
  match runtime {
    Ok(runtime) => runtime.block_on(async move { main_server(serve).await }),
    Err(err) => Err(err.into()),
  }
}

async fn main_server(serve: Serve) -> anyhow::Result<()> {
  let ServerHandle {
    server,
    shutdown,
    ready_rx: _ready_rx,
  } = build_server_handle(serve.clone().into())?;
  let mut ctx = SharedContextRw::new_shared_rw(serve.into()).await?;
  let app = build_routes(ctx.clone());
  let server_async = tokio::spawn(async move {
    let callback: Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send + 'static> = Box::new(|| {
      async move {
        if let Err(err) = ctx.try_stop().await {
          tracing::warn!(err = ?err, "error stopping llama context");
        }
      }
      .boxed()
    });
    match server.start_new(app, Some(callback)).await {
      Ok(()) => Ok(()),
      Err(err) => {
        tracing::error!(err = ?err, "server encountered an error");
        Err(err)
      }
    }
  });
  tokio::spawn(async move {
    shutdown_signal().await;
    shutdown
      .send(())
      .map_err(|_| anyhow::anyhow!("error sending shutdown signal on channel"))
      .context("sending shutdown signal to server")
      .unwrap();
  });
  (server_async.await?)?;
  Ok(())
}