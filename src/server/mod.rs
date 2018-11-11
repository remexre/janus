mod discord_parser;
mod discord_side;
mod irc_side;

use self::{discord_side::start_discord, irc_side::start_irc};
use crate::config::Config;
use failure::{format_err, Error};
use futures::{
    stream::{iter_ok, Stream},
    sync::mpsc::unbounded,
    Future, Sink,
};
use std::sync::Arc;

pub fn run(discord_token: &str) -> impl Future<Item = (), Error = Error> {
    let (discord_send, discord_send_recv) = unbounded();
    let (discord_recv_send, discord_recv) = unbounded();
    let (irc_send, irc_send_recv) = unbounded();
    let (irc_recv_send, irc_recv) = unbounded();

    let discord_side = start_discord(&discord_token, discord_send, discord_recv);
    let irc_side = start_irc(Config::irc_config(), irc_send, irc_recv);
    let discord_to_irc = discord_send_recv
        .map_err(|_| format_err!("Discord hung up?"))
        .map(|(chan, sender, msg)| {
            let msg = Arc::new(format_discord_for_irc(sender, msg));
            iter_ok(
                Config::irc_for_discord(chan)
                    .into_iter()
                    .map(move |chan| (chan, msg.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .forward(irc_recv_send.sink_map_err(|_| format_err!("Can't send to IRC")))
        .map(|_| ());
    let irc_to_discord = irc_send_recv
        .map_err(|_| format_err!("IRC hung up?"))
        .map(|(chan, sender, msg)| {
            let msg = Arc::new(format_irc_for_discord(sender, msg));
            iter_ok(Config::discord_for_irc(chan)).map(move |chan| (chan, msg.clone()))
        })
        .flatten()
        .forward(discord_recv_send.sink_map_err(|_| format_err!("Can't send to Discord")))
        .map(|_| ());

    discord_side
        .join4(irc_side, discord_to_irc, irc_to_discord)
        .map(|((), (), (), ())| ())
}

fn format_discord_for_irc(sender: String, msg: String) -> String {
    format!("{}: {}", sender, msg)
}

fn format_irc_for_discord(sender: String, msg: String) -> String {
    format!("__**{}**__: {}", sender, msg)
}
