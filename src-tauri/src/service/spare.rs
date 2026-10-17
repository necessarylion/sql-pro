//! One spare connection that is open already, so a new tab opens at once.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::model::ConnectionConfig;
use crate::repository::{Repository, RepositoryFactory, SshTunnel};

/// An open connection and its tunnel. The tunnel must live as long as the connection.
pub(super) type Opened = (Arc<dyn Repository>, Option<Arc<SshTunnel>>);

/// The spare connection of one group of tabs: the first tab and all its copies.
/// The whole group shares one spare, so the server gets only one extra connection.
// ponytail: this costs one extra idle connection per open server. It makes a new tab fast.
#[derive(Default)]
pub(super) struct Spare {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The spare, with the config it was opened with.
    ready: Option<(ConnectionConfig, Opened)>,
    /// A spare is opening now. Only one opens at a time.
    warming: bool,
    /// The last tab of the group is closed. No spare is kept any more.
    closed: bool,
}

impl Spare {
    /// Gives the spare when it has the same config and still answers. Else `None`.
    pub(super) async fn take(&self, config: &ConnectionConfig) -> Option<Opened> {
        let (spare_config, (repository, tunnel)) = self.state.lock().ready.take()?;
        // An idle connection can die (server timeout, NAT). One cheap query checks it.
        if spare_config == *config && repository.query("SELECT 1").await.is_ok() {
            return Some((repository, tunnel));
        }
        repository.close().await;
        None
    }

    /// Starts to open a spare in the background, when none is ready or opening.
    /// A failure is silent. Then the next new tab opens the slow way.
    pub(super) fn refill(
        self: &Arc<Self>,
        factory: RepositoryFactory,
        config: ConnectionConfig,
        tunnel: Option<Arc<SshTunnel>>,
    ) {
        {
            let mut state = self.state.lock();
            if state.closed || state.warming || state.ready.is_some() {
                return;
            }
            state.warming = true;
        }
        let spare = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            spare.warm(factory, config, tunnel).await;
        });
    }

    async fn warm(
        &self,
        factory: RepositoryFactory,
        config: ConnectionConfig,
        tunnel: Option<Arc<SshTunnel>>,
    ) {
        let opened = factory.create(&config, tunnel).await;
        let late = {
            let mut state = self.state.lock();
            state.warming = false;
            match opened {
                // The group closed while this opened. Close it, do not keep it.
                Ok(opened) if state.closed => Some(opened),
                Ok(opened) => {
                    state.ready = Some((config, opened));
                    None
                }
                Err(_) => None,
            }
        };
        if let Some((repository, _tunnel)) = late {
            repository.close().await;
        }
    }

    /// Closes the spare that is ready now. A later `refill` opens a new one.
    pub(super) async fn discard(&self) {
        let ready = self.state.lock().ready.take();
        if let Some((_, (repository, _tunnel))) = ready {
            repository.close().await;
        }
    }

    /// Closes the spare and keeps no new one. The spare also lets go of the tunnel.
    pub(super) async fn close(&self) {
        let ready = {
            let mut state = self.state.lock();
            state.closed = true;
            state.ready.take()
        };
        if let Some((_, (repository, _tunnel))) = ready {
            repository.close().await;
        }
    }

    #[cfg(test)]
    pub(super) fn ready(&self) -> Option<Arc<dyn Repository>> {
        let state = self.state.lock();
        state
            .ready
            .as_ref()
            .map(|(_, (repository, _))| Arc::clone(repository))
    }
}
