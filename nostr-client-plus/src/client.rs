use crate::basic_ws::BasicWS;
use crate::client_command::ClientCommand;
use crate::close::Close;
use crate::event::UnsignedEvent;
use crate::request::Request;
use anyhow::{anyhow, Result};
use nostr_crypto::sender_signer::SenderSigner;
use nostr_crypto::signer::Signer;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender as NostrSender;
use nostr_plus_common::types::SubscriptionId;
use std::collections::HashMap;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::tungstenite::Message;

const RETRIES: u16 = 60; // WS reconnect attempts

pub struct Client {
    signer: SenderSigner,
    int_tx: Option<UnboundedSender<ClientCommand>>,
    // ToDo: keep track of subscriptions
}

impl Client {
    pub fn new(signer: SenderSigner) -> Self {
        Self {
            signer,
            int_tx: None,
        }
    }

    pub async fn connect(&mut self, url: &str) -> Result<()> {
        self._connect(url, None).await
    }

    pub async fn connect_with_channel(
        &mut self,
        url: &str,
    ) -> Result<UnboundedReceiver<RelayMessage>> {
        let (ext_tx, ext_rx) = mpsc::unbounded_channel();
        self._connect(url, Some(ext_tx)).await.map(|_| ext_rx)
    }

    async fn _connect(
        &mut self,
        url: &str,
        sink: Option<UnboundedSender<RelayMessage>>,
    ) -> Result<()> {
        // If already connected, ignore. ToDo: handle this better
        if self.int_tx.is_some() {
            return Err(anyhow!("Already connected"));
        }

        // Warning: both sides of this channel are used in a tokio select. This needs to be unbounded because
        // we cannot afford to wait on it in one of the two branches, otherwise we could block
        // indefinitely
        let (int_tx, mut int_rx): (
            UnboundedSender<ClientCommand>,
            UnboundedReceiver<ClientCommand>,
        ) = mpsc::unbounded_channel();

        // Create WS connection
        let mut basic_ws = BasicWS::new(url).await?;

        // Spawn listener
        let int_tx_clone = int_tx.clone();
        tokio::spawn(async move {
            // Keep track of subscriptions
            let mut subscriptions: HashMap<SubscriptionId, (Request, Option<u64>)> = HashMap::new();

            let mut need_reconnect = false;

            loop {
                tokio::select! {
                    maybe_ws_msg = basic_ws.read_msg() => {
                        tracing::debug!("Msg from websocket");
                        match maybe_ws_msg {
                            Some(msg) => match msg {
                                Ok(Message::Text(message)) => {
                                    tracing::debug!("Got Text message");
                                    Self::handle_incoming_message(message, sink.as_ref()).await
                                }
                                Ok(Message::Ping(_)) => {
                                    tracing::debug!("Received Ping request");  // tungstenite handles Pong already
                                }
                                Ok(Message::Close(c)) => tracing::warn!("Server closed connection: {:?}", c), // ToDo: do something
                                Ok(Message::Binary(msg)) => {
                                    tracing::info!("Received Binary message");
                                    sink.as_ref().map(|ext_tx| {
                                        if ext_tx.send(RelayMessage::Binary(msg)).is_err() {
                                            tracing::error!("Cannot send binary to relay channel");
                                        }
                                    });
                                }
                                Ok(_) => tracing::warn!("Unsupported message"),
                                Err(err) => tracing::error!("Do something about {}", err), // ToDo: handle error
                            }
                            None => {
                                tracing::debug!("Client websocket probably closed");
                                need_reconnect = true;
                            }
                        }
                    }
                    maybe_int_msg = int_rx.recv() => {
                        tracing::debug!("Internal channel message");
                        match maybe_int_msg {
                            Some(message) => match message {
                                ClientCommand::Req((req, since_offset)) => {
                                    tracing::debug!("Subscription: {:?}", req);
                                    let req_str = req.to_string();
                                    subscriptions.insert(req.subscription_id.clone(), (req, since_offset));
                                    if basic_ws.send_msg(Message::from(req_str)).await.is_err() {
                                        tracing::error!("Req: websocket error");
                                        need_reconnect = true;
                                    }
                                }
                                ClientCommand::Event(event) => {
                                    tracing::debug!("Sending event of type: {}", event.kind());
                                    if basic_ws.send_msg(Message::from(event.to_string())).await.is_err() {
                                        tracing::error!("Event: websocket error");
                                        need_reconnect = true;
                                    }
                                }
                                ClientCommand::Close(close_msg) => {
                                    tracing::warn!("Send Close message");
                                    if basic_ws.send_msg(Message::from(close_msg.to_string())).await.is_err() {
                                        tracing::error!("Close subscription: websocket error");
                                        need_reconnect = true;
                                    } else {
                                        subscriptions.remove(&close_msg.subscription_id);
                                    }
                                }
                                ClientCommand::Binary(message) => {
                                    tracing::info!("Sending Binary message");
                                    // Note: only send_binary sends this message and we check there
                                    //  that it is a Message::Binary. But not really future-proof.
                                    if basic_ws.send_msg(message).await.is_err() {
                                        tracing::error!("Binary send: websocket error");
                                        need_reconnect = true;
                                    }
                                }
                            }
                            None => {
                                tracing::debug!("Client unrecoverable error: broken internal channel"); // ToDo: try to recover
                                break;
                            }
                        }
                    }
                }
                if need_reconnect {
                    let mut counter = 0;
                    while counter < RETRIES {
                        tracing::warn!("Trying to reconnect: attempt {}", counter);
                        match basic_ws.reconnect().await {
                            Ok(_) => {
                                need_reconnect = false;
                                break;
                            }
                            Err(_) => {}
                        }
                        counter += 1;
                        sleep(Duration::from_secs(2)).await
                    }
                    // if still not reconnected, break
                    // otherwise resubscribe to everything
                    if need_reconnect {
                        break;
                    } else {
                        let current_time = chrono::Utc::now().timestamp() as u64;
                        for (_, (ref mut req, since_offset)) in &mut subscriptions {
                            if let Some(offset) = since_offset {
                                let timestamp = current_time - *offset;
                                for filter in &mut req.filters {
                                    filter.since = Some(timestamp);
                                }
                            }
                            if let Err(err) =
                                int_tx_clone.send(ClientCommand::Req((req.clone(), *since_offset)))
                            {
                                tracing::error!("While resubscribing: {err}");
                            }
                        }
                    }
                }
            }
            if let Some(sink) = sink {
                if sink.send(RelayMessage::Disconnected).is_err() {
                    tracing::error!("Cannot send Disconnected message to user");
                }
            }
        });

        self.int_tx = Some(int_tx);

        Ok(())
    }

    async fn handle_incoming_message(msg: String, sink: Option<&UnboundedSender<RelayMessage>>) {
        match serde_json::from_str::<RelayMessage>(&msg) {
            Ok(relay_msg) => match sink {
                None => tracing::warn!("Unhandled: {:?}", msg),
                Some(sender) => {
                    if let Err(e) = sender.send(relay_msg) {
                        tracing::error!("{}", e);
                    }
                }
            },
            Err(err) => tracing::error!("ERROR ({}) with msg: {}", err, msg),
        }
    }

    pub async fn publish(&self, event: UnsignedEvent) -> Result<()> {
        let int_tx = self._get_send_channel()?;
        let event = event.sign(&self.signer)?;
        int_tx.send(ClientCommand::Event(event))?;
        Ok(())
    }

    pub fn sender(&self) -> NostrSender {
        match &self.signer {
            SenderSigner::Schnorr(signer) => {
                NostrSender::SchnorrPubKey(signer.private.verifying_key().to_bytes().into())
            }
            SenderSigner::Eoa(signer) => NostrSender::EoaAddress(signer.address()),
        }
    }

    pub fn try_sign(&self, msg: &[u8; 32]) -> Result<Vec<u8>> {
        match &self.signer {
            SenderSigner::Schnorr(signer) => signer.try_sign(msg),
            SenderSigner::Eoa(signer) => signer.try_sign(msg),
        }
    }

    // ToDo: reconnect_since is a terrible hack to let users specify an offset from "now"
    //  to use in the `since` field when reconnecting. If None, use the old `since` value.
    pub async fn subscribe(&self, req: Request, reconnect_since: Option<u64>) -> Result<()> {
        match &self.int_tx {
            Some(int_tx) => {
                int_tx.send(ClientCommand::Req((req, reconnect_since)))?;
            }
            None => return Err(anyhow!("Subscribe: missing websocket")),
        }
        Ok(())
    }

    pub async fn close_subscription(&self, sub_id: SubscriptionId) -> Result<()> {
        let close_msg = Close::new(sub_id);
        match &self.int_tx {
            Some(int_tx) => {
                int_tx.send(ClientCommand::Close(close_msg))?;
            }
            None => return Err(anyhow!("Close subscription: missing websocket")),
        }
        Ok(())
    }

    /// This function is a general purpose function to give a chance to users
    /// to send their own binary format
    pub async fn send_binary(&self, message: Message) -> Result<()> {
        match &message {
            Message::Binary(_) => {
                let int_tx = self._get_send_channel()?;
                int_tx.send(ClientCommand::Binary(message))?;
                Ok(())
            }
            _ => Err(anyhow!("Invalid message variant")),
        }
    }

    fn _get_send_channel(&self) -> Result<&UnboundedSender<ClientCommand>> {
        match &self.int_tx {
            Some(tx) => Ok(tx),
            None => Err(anyhow!("Missing internal channel")),
        }
    }
}
