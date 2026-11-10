//! Reads what decides who may use the daemon from Moonraker, and publishes it.
//!
//! One task follows the control connection that [`crate::moonraker`] publishes. It reads the
//! lists on each new connection and again on a timer, publishes them, and publishes them as
//! unknown when the connection goes. It never touches the connection's supervision.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, watch};

use super::policy::{parse_hostname, parse_login_required};
use super::{AccessState, Policy};
use crate::moonraker::rpc::{Rpc, RpcError};
use crate::moonraker::{Timings, publish_access};

/// The lists read from Moonraker, and what is remembered across connections for logging and for
/// the one fact that must outlive a connection.
#[derive(Default)]
pub struct AccessWatcher {
    policy: Option<Policy>,
    hostname: Option<String>,
    login_required: Option<bool>,
    logged: Option<(Vec<String>, Vec<String>, Option<String>)>,
    /// The reason the policy was last unreadable, so it is logged once.
    unreadable: Option<String>,
}

impl AccessWatcher {
    /// Forgets what the last connection read and publishes the access state as unknown, with the
    /// last login requirement. What outlives a connection stays.
    fn clear(&mut self, sink: &watch::Sender<AccessState>) {
        self.policy = None;
        self.hostname = None;
        self.publish(sink);
    }

    /// Whether the lists and the host name are both read, so the slow poll is enough.
    fn settled(&self) -> bool {
        self.policy.is_some() && self.hostname.is_some()
    }

    /// Reads what decides who may use the daemon, and publishes it when it changed.
    ///
    /// `login_required` is read every time, because creating an account changes it without a
    /// restart. `server.config` is read while the lists are unknown, and the host name until
    /// one arrives. A Moonraker error or timeout leaves the lists unknown and keeps the
    /// connection.
    ///
    /// # Errors
    /// A refusal or a broken connection.
    async fn refresh(
        &mut self,
        rpc: &Rpc,
        timings: &Timings,
        sink: &watch::Sender<AccessState>,
    ) -> Result<(), RpcError> {
        let info = tolerate(rpc.call("access.info", None, timings.request).await)?;
        let login = info.as_ref().and_then(parse_login_required);
        if let Some(login) = login {
            self.note_login(login);
        }
        let mut reason = None;
        match login {
            None => {
                self.policy = None;
                reason = Some("access.info gave no login_required".to_owned());
            }
            Some(login) if self.policy.is_none() => {
                let config = tolerate(rpc.call("server.config", None, timings.configfile).await)?;
                match config {
                    None => reason = Some("server.config gave no answer".to_owned()),
                    Some(config) => match Policy::build(&config, login, self.hostname.as_deref()) {
                        Ok(policy) => self.policy = Some(policy),
                        Err(e) => reason = Some(format!("server.config: {e}")),
                    },
                }
            }
            Some(login) => {
                if let Some(policy) = self.policy.as_mut()
                    && policy.logins_required() != login
                {
                    *policy = policy.with_logins_required(login);
                }
            }
        }
        if self.hostname.is_none() && self.policy.is_some() {
            let info = tolerate(rpc.call("printer.info", None, timings.request).await)?;
            if let Some(name) = info.as_ref().and_then(parse_hostname) {
                self.hostname = Some(name.to_lowercase());
                if let Some(policy) = self.policy.as_mut() {
                    *policy = policy.with_hostname(&name);
                }
            }
        }
        self.unreadable = match reason {
            Some(reason) => {
                if self.unreadable.as_deref() != Some(reason.as_str()) {
                    tracing::warn!(
                        "cannot read Moonraker's access lists ({reason}); only loopback clients \
                         are admitted until they are read"
                    );
                }
                Some(reason)
            }
            None => None,
        };
        self.publish(sink);
        Ok(())
    }

    fn note_login(&mut self, login: bool) {
        if self.login_required != Some(login) && (login || self.login_required.is_some()) {
            if login {
                tracing::warn!(
                    "Moonraker requires logins; every request is refused until it does not"
                );
            } else {
                tracing::warn!("Moonraker no longer requires logins");
            }
        }
        self.login_required = Some(login);
    }

    /// Publishes the lists when they are read, else unknown with the last login requirement. Logs
    /// the lists when they changed.
    fn publish(&mut self, sink: &watch::Sender<AccessState>) {
        let state = match self.policy.clone() {
            Some(policy) => {
                self.log_policy(&policy);
                AccessState::Known(std::sync::Arc::new(policy))
            }
            None => AccessState::Unknown {
                logins_required: self.login_required == Some(true),
            },
        };
        publish_access(sink, state);
    }

    fn log_policy(&mut self, policy: &Policy) {
        let key = (
            policy.trusted_clients().to_vec(),
            policy.cors_domains().to_vec(),
            policy.hostname().map(str::to_owned),
        );
        if self.logged.as_ref() == Some(&key) {
            return;
        }
        let lists_changed = self
            .logged
            .as_ref()
            .is_none_or(|(trusted, cors, _)| *trusted != key.0 || *cors != key.1);
        tracing::info!(
            "Moonraker's access lists: trusted_clients {:?}, cors_domains {:?}, hostname {:?}",
            key.0,
            key.1,
            key.2
        );
        if lists_changed {
            for ignored in policy.ignored() {
                tracing::warn!(
                    "ignoring {:?} in Moonraker's {:?}: {}",
                    ignored.entry,
                    ignored.list,
                    ignored.reason
                );
            }
        }
        self.logged = Some(key);
    }
}

/// Runs until the supervisor that publishes `control` is gone. Moonraker refusing an access call
/// is sent on `refused`, so the supervisor can report that it refused the daemon.
pub async fn run(
    mut control: watch::Receiver<Option<Rpc>>,
    sink: Arc<watch::Sender<AccessState>>,
    timings: Timings,
    refused: mpsc::UnboundedSender<()>,
) {
    let mut watcher = AccessWatcher::default();
    loop {
        let rpc = loop {
            if let Some(rpc) = control.borrow_and_update().clone() {
                break rpc;
            }
            if control.changed().await.is_err() {
                return;
            }
        };
        loop {
            if let Err(e) = watcher.refresh(&rpc, &timings, &sink).await {
                tracing::debug!("cannot read Moonraker's access settings: {e}");
                if e.is_unauthorized() {
                    let _ = refused.send(());
                }
                watcher.clear(&sink);
            }
            let wait = if watcher.settled() {
                timings.poll_ready
            } else {
                timings.poll_not_ready
            };
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                changed = control.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    break;
                }
            }
        }
        watcher.clear(&sink);
    }
}

/// An answer, or `None` when Moonraker answered with an error or too slowly. A refusal or a
/// broken connection is the error.
fn tolerate(answer: Result<Value, RpcError>) -> Result<Option<Value>, RpcError> {
    match answer {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.is_unauthorized() => Err(e),
        Err(RpcError::Timeout | RpcError::Moonraker { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}
