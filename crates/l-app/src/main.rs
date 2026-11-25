//! Monocord — a Discord client powered by the real web app.
//!
//! This binary is the client itself: a `wry`/`WebView2` window on
//! `discord.com` with the Monocord plugin + theme bridge injected. The
//! user's real Discord account handles auth inside Discord's own client.

mod discord_web;
mod monocord;

fn main() {
    discord_web::run_client();
}
