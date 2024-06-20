use std::collections::BTreeMap;

use reqwest::Method;

use crate::{connection::Connection, model::ReadyEvent, Discord, Error, Object, Result};

pub trait ConnectExt {
    /// Establish a websocket connection over which events can be received.
    ///
    /// This method waits for and returns the `ReadyEvent` sent by Discord upon establishing the
    /// connection, which contains the initial state as seen by the client.
    ///
    /// See `connect_sharded` if you want to use guild sharding.
    async fn connect(&self) -> Result<(Connection, ReadyEvent)>;

    /// Establish a sharded websocket connection over which events can be received.
    /// The `shard_id` is indexed at 0 while `total_shards` is indexed at 1.
    ///
    /// This method waits for and returns the `ReadyEvent` sent by Discord upon establishing the
    /// connection, which contains the initial state as seen by the client.
    ///
    /// See `connect` if you do not want to use guild sharding.
    async fn connect_sharded(
        &self,
        shard_id: u8,
        total_shards: u8,
    ) -> Result<(Connection, ReadyEvent)>;

    /// Retrieves the number of guild shards Discord suggests to use based on the number of guilds.
    /// This endpoint is only available for bots.
    async fn suggested_shard_count(&self) -> Result<u8>;
}

impl ConnectExt for Discord {
    async fn connect(&self) -> Result<(Connection, ReadyEvent)> {
        todo!()
    }

    async fn connect_sharded(
        &self,
        _shard_id: u8,
        _total_shards: u8,
    ) -> Result<(Connection, ReadyEvent)> {
        todo!()
    }

    async fn suggested_shard_count(&self) -> Result<u8> {
        let mut response = self
            .empty_request("/gateway/bot", Method::GET)
            .await?
            .json::<Object>()
            .await?;

        match response.remove("shards") {
            Some(value) => match value.as_u64() {
                Some(shards) => Ok(shards as u8),
                None => Err(Error::Decode("Invalid \"shards\"", value)),
            },
            None => Err(Error::Decode(
                "suggested_shard_count missing \"shards\"",
                serde_json::Value::Object(response),
            )),
        }
    }
}

/// Fetch the gateway URL to connect to.
async fn get_gateway_url(client: &Discord) -> Result<String> {
    let mut response: BTreeMap<String, String> = client
        .empty_request("/gateway", Method::GET)
        .await?
        .json()
        .await?;

    match response.remove("url") {
        Some(url) => Ok(url),
        None => Err(Error::Protocol(
            "Response missing \"url\" in Discord::get_gateway_url()",
        )),
    }
}
