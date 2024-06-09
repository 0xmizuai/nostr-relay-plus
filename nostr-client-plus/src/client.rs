use crate::client_command::ClientCommand;
use crate::event::Event;
use crate::request::Request;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug)]
pub enum ClientError {
    Generic,
}

pub struct Client {
    sender: Option<Sender<ClientCommand>>,
    // ToDo: keep track of subscriptions
}

impl Client {
    pub fn new() -> Self {
        Self { sender: None }
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), ClientError> {
        // If already connected, ignore. ToDo: handle this better
        // if self.relay.is_some() {
        //     return Ok(());
        // }

        let (sender, mut receiver): (Sender<ClientCommand>, Receiver<ClientCommand>) =
            mpsc::channel(32);

        let (socket, resp) = match connect_async(url).await {
            Ok((socket, response)) => (socket, response),
            Err(_) => return Err(ClientError::Generic),
        };
        println!("Connection established: {:?}", resp);

        let (mut write, mut read) = socket.split();

        // Spawn listener
        tokio::spawn(async move {
            while let Some(Ok(message)) = read.next().await {
                println!("Received: {:?}", message);
            }
        });

        // Spawn sender, reading commands from internal channel
        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                println!("Command from channel {:?}", message);
                match message {
                    ClientCommand::Req(req) => {
                        if write.send(Message::from(req.to_string())).await.is_err() {
                            eprintln!("Req: websocket error"); // ToDo: do something better
                            break;
                        }
                    }
                    ClientCommand::Event(event) => {
                        if write.send(Message::from(event.to_string())).await.is_err() {
                            eprintln!("Event: websocket error"); // ToDo: do something better
                            break;
                        }
                    }
                }
            }
        });

        self.sender = Some(sender);

        Ok(())
    }

    pub async fn publish(&self, event: Event) -> Result<(), ClientError> {
        match &self.sender {
            Some(sender) => {
                sender.send(ClientCommand::Event(event)).await.unwrap();
            }
            None => return Err(ClientError::Generic),
        }
        Ok(())
    }

    pub async fn subscribe(&self, req: Request) -> Result<(), ClientError> {
        match &self.sender {
            Some(sender) => {
                sender.send(ClientCommand::Req(req)).await.unwrap(); // ToDo: remove unwrap
            }
            None => return Err(ClientError::Generic),
        }
        Ok(())
    }
}
