use anyhow::{Context, Error, Result};
use ractor::{Actor, ActorCell, ActorRef};
use regex::Regex;
use tokio::macros::support::Future;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, info};

pub struct ServiceManager {
    actors: Vec<ActorCell>,
    tracker: TaskTracker,
    token: CancellationToken,
    actors_sender: mpsc::Sender<ActorCell>,
}

impl ServiceManager {
    pub fn new() -> Self {
        let (actors_sender, actors_receiver) = mpsc::channel(1);

        let service_manager = Self {
            actors: Vec::new(),
            tracker: TaskTracker::new(),
            token: CancellationToken::new(),
            actors_sender,
        };

        service_manager.spawn_cleaning_task(actors_receiver);
        service_manager
    }

    pub async fn spawn_actor<A>(
        &mut self,
        actor: A,
        args: A::Arguments,
    ) -> Result<ActorRef<A::Msg>, Error>
    where
        A: Actor,
    {
        let name = Some(simplify_type_name(std::any::type_name::<A>()));
        let (actor_ref, actor_handle) = Actor::spawn(name, actor, args).await?;
        self.tracker.reopen();
        self.tracker.spawn(actor_handle);
        self.tracker.close();

        self.actors.push(actor_ref.get_cell());
        self.actors_sender
            .send(actor_ref.get_cell())
            .await
            .expect("Failed to send actor to cleanup task");

        Ok(actor_ref)
    }

    pub async fn spawn_blocking_actor<A>(
        &mut self,
        actor: A,
        args: A::Arguments,
    ) -> Result<ActorRef<A::Msg>, Error>
    where
        A: Actor,
    {
        let name = Some(simplify_type_name(std::any::type_name::<A>()));
        let (actor_ref, actor_handle) = Actor::spawn(name, actor, args).await?;
        self.tracker.reopen();
        self.tracker.spawn_blocking(move || {
            match tokio::runtime::Runtime::new().context("Failed to create a new Runtime") {
                Ok(rt) => rt.block_on(async move { actor_handle.await }),
                Err(e) => {
                    error!("Failed to create a new Runtime: {}", e);
                    Ok(())
                }
            }
        });
        self.tracker.close();

        self.actors.push(actor_ref.get_cell());
        self.actors_sender
            .send(actor_ref.get_cell())
            .await
            .expect("Failed to send actor to cleanup task");

        Ok(actor_ref)
    }

    // Spawn through a function that receives a cancellation token
    #[allow(dead_code)]
    pub fn spawn_service<F, Fut>(&self, task: F) -> JoinHandle<()>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send,
    {
        let token = self.token.clone();
        self.tracker.reopen();
        let join_handle = self.tracker.spawn(async move {
            let token_clone = token.clone();
            let task_fut = task(token);
            if let Err(e) = task_fut.await {
                error!("Task failed: {}", e);
                token_clone.cancel();
            }
        });
        self.tracker.close();
        join_handle
    }

    // Spawn through a function that receives a cancellation token. This function will be called in a new thread
    pub fn spawn_blocking_service<F, Fut>(&self, task: F) -> JoinHandle<()>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send,
    {
        let token = self.token.clone();
        self.tracker.reopen();
        let join_handle = self.tracker.spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create a new Runtime");
            let token_clone = token.clone();
            rt.block_on(async move {
                let result = task(token).await;
                if let Err(e) = result {
                    error!("Task failed: {}", e);
                    token_clone.cancel();
                }
            });
        });
        self.tracker.close();
        join_handle
    }

    // Wait until all actors and services are done
    pub async fn listen_stop_signals(&self) -> Result<()> {
        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = self.tracker.wait() => {},
            _ = signal::ctrl_c() => {
                info!("Starting graceful termination, from ctrl-c");
            },
            _ = terminate => {
                info!("Starting graceful termination, from terminate signal");
            },
        }

        self.stop().await;

        Ok(())
    }

    // Stop all actors and services
    pub async fn stop(&self) {
        self.token.cancel();
        info!("Wait for all tasks to complete after the cancel");
        self.tracker.wait().await;
        info!("All tasks completed bye bye");
    }

    fn spawn_cleaning_task(&self, mut actors_receiver: mpsc::Receiver<ActorCell>) {
        let token_clone = self.token.clone();

        tokio::spawn(async move {
            let mut actors: Vec<ActorCell> = Vec::new();
            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        debug!("ServiceManager is being dropped, cancelling all tasks");
                        for actor in &actors {
                            debug!("Stopping actor");
                            actor.stop(Some("ServiceManager is being dropped".to_string()));
                            debug!("Actor stopped");
                        }

                        break;
                    }
                    Some(actor_ref) = actors_receiver.recv() => {
                        debug!("Received actor to cleanup");
                        actors.push(actor_ref);
                    }
                }
            }
        });
    }
}

impl Drop for ServiceManager {
    fn drop(&mut self) {
        if !self.token.is_cancelled() {
            debug!("ServiceManager is being dropped, cancelling all tasks");
            self.token.cancel();
        }
    }
}

fn simplify_type_name(input: &str) -> String {
    let mut result = input.to_string();
    // Match segments starting with lowercase followed by any of the specified delimiters
    let regex = Regex::new(r"\b[a-z]\w*(::|<)").unwrap();

    // As long as there's a match, keep replacing
    while let Some(mat) = regex.find(&result) {
        // Calculate the replacement range to keep the delimiter
        let range = mat.start()..mat.end() - 2;
        // Replace the matched segment with an empty string, effectively removing it
        result.replace_range(range, "");
    }

    result.replace(":", "")
}
