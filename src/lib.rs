//! Client library for the [Discord](https://discord.com) API.
//!
//! The Discord API can be divided into three main components: the RESTful API
//! to which calls can be made to take actions, a websocket-based permanent
//! connection over which state updates are received, and the voice calling
//! system.
//!
//! Log in to Discord with `Discord::new`, `new_cache`, or `from_bot_token` as appropriate.
//! The resulting value can be used to make REST API calls to post messages and manipulate Discord
//! state. Calling `connect()` will open a websocket connection, through which events can be
//! received. These two channels are enough to write a simple chatbot which can
//! read and respond to messages.
//!
//! For more in-depth tracking of Discord state, a `State` can be seeded with
//! the `ReadyEvent` obtained when opening a `Connection` and kept updated with
//! the events received over it.
//!
#![cfg_attr(
    not(feature = "voice"),
    doc = "*<b>NOTE</b>: The library has been compiled without voice support.*"
)]
//! To join voice servers, call `Connection::voice` to get a `VoiceConnection` and use `connect`
//! to join a channel, then `play` and `stop` to control playback. Manipulating deaf/mute state
//! and receiving audio are also possible.
//!
//! For examples, see the `examples` directory in the source tree.

#![warn(missing_docs, missing_debug_implementations)]

type Object = serde_json::Map<String, serde_json::Value>;

mod connection;
mod error;
mod ratelimit;
mod state;

mod rest;
pub use rest::*;

macro_rules! cdn_concat {
    ($e:expr) => {
        // Out of everything, only the CDN still uses the old domain.
        concat!("https://cdn.discordapp.com", $e)
    };
}

/// Struct and enum definitions of values in the Discord model.
pub mod model {
    mod event;
    pub use self::event::*;

    mod rest;
    pub use rest::*;

    mod live;
    pub use live::*;
}

// #[cfg(feature = "voice")]
// pub mod voice;

#[macro_use]
mod serial;
pub mod builders;

pub use error::{Error, Result};
pub use state::{ChannelRef, State};

const API_BASE: &'static str = "https://discord.com/api/v6";

/// Read an image from a file into a string suitable for upload.
///
/// If the file's extension is `.png`, the claimed media type will be `image/png`, or `image/jpg`
/// otherwise. Note that Discord may convert the image to JPEG or another format after upload.
pub fn read_image<P: AsRef<::std::path::Path>>(path: P) -> Result<String> {
    use std::io::Read;
    let path = path.as_ref();
    let mut vec = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut vec)?;
    Ok(format!(
        "data:image/{};base64,{}",
        if path.extension() == Some("png".as_ref()) {
            "png"
        } else {
            "jpg"
        },
        base64::encode(&vec),
    ))
}
