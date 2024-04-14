use std::collections::HashMap;

use anyhow::{anyhow, Result};
use nostr_surreal_db::message::{events::Event, subscription::Subscription};

use super::LocalState;

/// A subscription identifier has a maximum length
const MAX_SUBSCRIPTION_ID_LEN: usize = 256;

// On subscriptions
impl LocalState {

    #[must_use]
    pub fn subscriptions(&self) -> &HashMap<String, Subscription> {
        &self.subscriptions
    }

    pub fn is_interested(&self, e: &Event) -> Result<String> {
        for (id, sub) in self.subscriptions.iter() {
            if sub.is_interested(&e).is_ok() {
                return Ok(id.clone());
            }
        }
        Err(anyhow!("not interested"))
    }

    /// Check if the given subscription already exists
    #[must_use]
    pub fn has_subscription(&self, sub: &Subscription) -> bool {
        self.subscriptions.values().any(|x| x == sub)
    }

    /// Add a new subscription for this connection.
    /// # Errors
    ///
    /// Will return `Err` if the client has too many subscriptions, or
    /// if the provided name is excessively long.
    pub fn subscribe(&mut self, s: Subscription) -> Result<()> {
        let k = &s.id;
        let sub_id_len = k.len();
        // prevent arbitrarily long subscription identifiers from
        // being used.
        if sub_id_len > MAX_SUBSCRIPTION_ID_LEN {
            tracing::debug!(
                "ignoring sub request with excessive length: ({})",
                sub_id_len
            );
            return Err(anyhow!("SubIdMaxLengthError"));
        }
        // check if an existing subscription exists, and replace if so
        if self.subscriptions.contains_key(k) {
            self.subscriptions.remove(k);
            self.subscriptions.insert(k.clone(), s.clone());
            tracing::trace!(
                "replaced existing subscription (cid: {}, sub: {:?})",
                self.client_ip_addr,
                &s.id
            );
            return Ok(());
        }

        // check if there is room for another subscription.
        if self.subscriptions.len() >= self.max_subs {
            return Err(anyhow!("SubMaxExceededError"));
        }
        // add subscription
        self.subscriptions.insert(k.clone(), s);
        tracing::trace!(
            "registered new subscription, currently have {} active subs (cid: {})",
            self.subscriptions.len(),
            self.client_ip_addr,
        );
        Ok(())
    }

    /// Remove the subscription for this connection.
    pub fn unsubscribe(&mut self, c: &str) {
        // TODO: return notice if subscription did not exist.
        self.subscriptions.remove(&c.to_owned());
        tracing::trace!(
            "removed subscription, currently have {} active subs (cid: {})",
            self.subscriptions.len(),
            self.client_ip_addr,
        );
    }
}