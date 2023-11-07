use chrono::ParseError as ChronoError;
#[cfg(feature = "voice")]
use opus::Error as OpusError;
use reqwest::Error as ReqwestError;
use serde_json::Error as JsonError;
use serde_json::Value;
use std::error::Error as StdError;
use std::fmt::Display;
use std::io::Error as IoError;
use websockets::WebSocketError;

/// Discord API `Result` alias type.
pub type Result<T> = ::std::result::Result<T, Error>;

/// Discord API error type.
#[derive(Debug)]
pub enum Error {
    /// A `reqwest` crate error
    Reqwest(ReqwestError),
    /// A `chrono` crate error
    Chrono(ChronoError),
    /// A `serde_json` crate error
    Json(JsonError),
    /// A `websockets` crate error
    WebSocket(WebSocketError),
    /// A `std::io` module error
    Io(IoError),
    /// An error in the Opus library, with the function name and error code
    #[cfg(feature = "voice")]
    Opus(OpusError),
    /// A websocket connection was closed, possibly with a message
    Closed(Option<u16>, String),
    /// A json decoding error, with a description and the offending value
    Decode(&'static str, Value),
    /// A generic non-success response from the REST API
    Status(reqwest::StatusCode, Option<Value>),
    /// A rate limit error, with how many milliseconds to wait before retrying
    RateLimited(u64),
    /// A Discord protocol error, with a description
    Protocol(&'static str),
    /// A miscellaneous error, with a description
    Other(&'static str),
}

impl Error {
    #[doc(hidden)]
    pub async fn from_response(response: reqwest::Response) -> Error {
        let status = response.status();

        let value = response
            .bytes()
            .await
            .ok()
            .map(|b| serde_json::from_slice(&b).ok())
            .flatten();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            if let Some(Value::Object(ref map)) = value {
                if let Some(delay) = map.get("retry_after").and_then(|v| v.as_u64()) {
                    return Error::RateLimited(delay);
                }
            }
        }
        Error::Status(status, value)
    }
}

impl From<IoError> for Error {
    fn from(err: IoError) -> Error {
        Error::Io(err)
    }
}

impl From<ReqwestError> for Error {
    fn from(err: ReqwestError) -> Error {
        Error::Reqwest(err)
    }
}

impl From<ChronoError> for Error {
    fn from(err: ChronoError) -> Error {
        Error::Chrono(err)
    }
}

impl From<JsonError> for Error {
    fn from(err: JsonError) -> Error {
        Error::Json(err)
    }
}

impl From<WebSocketError> for Error {
    fn from(err: WebSocketError) -> Error {
        Error::WebSocket(err)
    }
}

#[cfg(feature = "voice")]
impl From<OpusError> for Error {
    fn from(err: OpusError) -> Error {
        Error::Opus(err)
    }
}

impl Display for Error {
    #[allow(deprecated)]
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::Reqwest(ref inner) => inner.fmt(f),
            Error::Chrono(ref inner) => inner.fmt(f),
            Error::Json(ref inner) => inner.fmt(f),
            Error::WebSocket(ref inner) => inner.fmt(f),
            Error::Io(ref inner) => inner.fmt(f),
            #[cfg(feature = "voice")]
            Error::Opus(ref inner) => inner.fmt(f),
            _ => f.write_str(self.description()),
        }
    }
}

impl StdError for Error {
    #[allow(deprecated)]
    fn description(&self) -> &str {
        match *self {
            Error::Reqwest(ref inner) => inner.description(),
            Error::Chrono(ref inner) => inner.description(),
            Error::Json(ref inner) => inner.description(),
            Error::WebSocket(ref inner) => inner.description(),
            Error::Io(ref inner) => inner.description(),
            #[cfg(feature = "voice")]
            Error::Opus(ref inner) => inner.description(),
            Error::Closed(_, _) => "Connection closed",
            Error::Decode(msg, _) | Error::Protocol(msg) | Error::Other(msg) => msg,
            Error::Status(status, _) => status
                .canonical_reason()
                .unwrap_or("Unknown bad HTTP status"),
            Error::RateLimited(_) => "Rate limited",
        }
    }

    fn cause(&self) -> Option<&dyn StdError> {
        match *self {
            Error::Reqwest(ref inner) => Some(inner),
            Error::Chrono(ref inner) => Some(inner),
            Error::Json(ref inner) => Some(inner),
            Error::WebSocket(ref inner) => Some(inner),
            Error::Io(ref inner) => Some(inner),
            #[cfg(feature = "voice")]
            Error::Opus(ref inner) => Some(inner),
            _ => None,
        }
    }
}

/// Extension trait for checking the status and discarding failed discord HTTP requests.
pub(crate) trait CheckStatus {
    /// Convert non-success hyper statuses to discord crate errors, tossing info.
    async fn check_status(self) -> Result<reqwest::Response>;
}

impl CheckStatus for reqwest::Result<reqwest::Response> {
    async fn check_status(self) -> Result<reqwest::Response> {
        let response = self?;
        if !response.status().is_success() {
            return Err(Error::from_response(response).await);
        }
        Ok(response)
    }
}

/// Extension trait for checking the status dumping unexpected discord HTTP requests.
pub(crate) trait StatusChecks {
    /// Validate a request that is expected to return 204 No Content and print
    /// debug information if it does not.
    async fn insure_no_content(self) -> Result<()>;
}

impl StatusChecks for reqwest::Response {
    async fn insure_no_content(self) -> Result<()> {
        if self.status() != reqwest::StatusCode::NO_CONTENT {
            debug!("Expected 204 No Content, got {}", self.status());

            for (header_name, header_value) in self.headers().iter() {
                debug!("Header: {}: {:?}", header_name, header_value);
            }

            let content = self.bytes().await?;
            debug!("Content: {:?}", content);
        }
        Ok(())
    }
}
