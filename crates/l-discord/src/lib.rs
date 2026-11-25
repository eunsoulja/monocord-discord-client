//! Discord integration for L: `OAuth2` (PKCE, public client), REST client with
//! central rate limiting, capabilities, demo provider, optional Gateway.

pub mod capabilities;
pub mod demo;
pub mod gateway;
pub mod oauth;
pub mod rest;
pub mod voice;

pub use capabilities::Capabilities;
pub use demo::DemoBackend;
pub use oauth::{OAuthConfig, OAuthTokens};
pub use rest::{DiscordRest, TokenKind};
