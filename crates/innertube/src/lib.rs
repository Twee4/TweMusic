//! InnerTube access for TweMusic: client identities, the HTTP transport, stream lookup and
//! small identifier helpers. It has no UI and no audio code, so it can be tested on its own.

pub mod browse;
pub mod clients;
pub mod ids;
pub mod models;
pub mod parse;
pub mod player;
pub mod related;
pub mod search;
pub mod transport;

use thiserror::Error;

pub use player::{is_allowed_stream_url, resolve, Stream};

#[derive(Error, Debug)]
pub enum InnerTubeError {
    #[error("Network error: {0}")]
    Http(#[from] reqwest::Error),
    /// YouTube answered but would not play the video (age gate, region, removed, ...).
    #[error("YouTube refused: {0}")]
    Refused(String),
    /// YouTube is asking this connection to prove it is not a bot ("Sign in to confirm you're
    /// not a bot"). It depends on the network address, so no client profile gets around it.
    #[error("YouTube is asking this connection to prove it isn't a bot, so it won't hand out streams right now. This is about your network address, not TweMusic. Try again later, switch networks, or turn off a VPN.")]
    BotCheck,
    #[error("No playable audio stream was found.")]
    NoFormat,
    /// Reserved for signature / throttling deciphering, which is not implemented yet.
    #[error("Could not decipher the stream address.")]
    CipherFailed,
    /// Every attempt failed before YouTube answered (no connection, DNS, timeout).
    #[error("Could not reach YouTube. Check your internet connection.")]
    Unreachable,
    #[error("{0}")]
    Api(String),
}
