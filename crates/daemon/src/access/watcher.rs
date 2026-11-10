//! Reads what decides who may use the daemon from Moonraker, and publishes it.
//!
//! The supervisor in [`crate::moonraker`] owns the connection and calls [`AccessWatcher::refresh`]
//! on it. The watcher keeps the policy of the current connection and what it needs to log each
//! change once.

use serde_json::Value;

use super::policy::{parse_hostname, parse_login_required};
use super::{AccessState, Policy};
use crate::moonraker::rpc::{Rpc, RpcError};
use crate::moonraker::{Publisher, Timings};

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
    /// Forgets what the last connection read. What outlives a connection stays.
    pub fn start_connection(&mut self) {
        self.policy = None;
        self.hostname = None;
    }

    /// Publishes the access state as unknown, with the last login requirement.
    pub fn clear(&mut self, publisher: &Publisher) {
        self.policy = None;
        self.hostname = None;
        self.publish(publisher);
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
    pub async fn refresh(
        &mut self,
        rpc: &Rpc,
        timings: &Timings,
        publisher: &Publisher,
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
        self.publish(publisher);
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
    fn publish(&mut self, publisher: &Publisher) {
        let state = match self.policy.clone() {
            Some(policy) => {
                self.log_policy(&policy);
                AccessState::Known(std::sync::Arc::new(policy))
            }
            None => AccessState::Unknown {
                logins_required: self.login_required == Some(true),
            },
        };
        publisher.set_access(state);
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
