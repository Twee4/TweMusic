//! InnerTube access for TweMusic: client identities, the HTTP transport, stream lookup and
//! small identifier helpers. It has no UI and no audio code, so it can be tested on its own.

pub mod clients;
pub mod ids;
pub mod player;
pub mod transport;

pub use player::{is_allowed_stream_url, resolve, Stream};
