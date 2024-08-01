use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

pub struct BasicWS {
    url: String,
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
}

impl BasicWS {
    pub async fn new(url: &str) -> Result<Self> {
        let (socket, resp) = match connect_async(url).await {
            Ok((socket, response)) => (socket, response),
            Err(err) => return Err(anyhow!("Cannot connect to {}: {}", url, err)),
        };
        println!("Connection established: {:?}", resp);

        Ok(Self {
            url: url.to_string(),
            socket,
        })
    }

    pub async fn read_msg(&mut self) -> Option<Result<Message>> {
        self.socket
            .next()
            .await
            .map(|result| result.map_err(|err| anyhow!("{err}")))
    }

    pub async fn send_msg(&mut self, msg: Message) -> Result<()> {
        match self.socket.send(msg.clone()).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!("{err}")),
        }
    }
}
