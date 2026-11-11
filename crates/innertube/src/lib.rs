//! InnerTube access for TweMusic: client identities, the HTTP transport, stream lookup and
//! small identifier helpers. It has no UI and no audio code, so it can be tested on its own.

pub mod clients;
pub mod ids;
pub mod player;
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
    #[error("No playable audio stream was found.")]
    NoFormat,
    /// Reserved for signature / throttling deciphering, which is not implemented yet.
    #[error("Could not decipher the stream address.")]
    CipherFailed,
    #[error("{0}")]
    Api(String),
}
