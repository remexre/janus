use crate::{config::Config, discord_side::start_discord, irc_side::start_irc};
use failure::{format_err, Error};
use futures::{
    stream::{iter_ok, Stream},
    sync::mpsc::unbounded,
    Future, Sink,
};

pub fn run(discord_token: &str) -> impl Future<Item = (), Error = Error> {
    let (discord_send, discord_send_recv) = unbounded();
    let (discord_recv_send, discord_recv) = unbounded();
    let (irc_send, irc_send_recv) = unbounded();
    let (irc_recv_send, irc_recv) = unbounded();

    let discord_side = start_discord(&discord_token, discord_send, discord_recv);
    let irc_side = start_irc(Config::irc_config(), irc_send, irc_recv);
    let discord_to_irc = discord_send_recv
        .map_err(|_| format_err!("Discord hung up?"))
        .map(|(chan, sender, msg): (u64, String, String)| {
            iter_ok(
                Config::irc_for_discord(chan)
                    .into_iter()
                    .map(move |chan| (chan, sender.clone(), msg.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .forward(irc_recv_send.sink_map_err(|_| format_err!("Can't send to IRC")))
        .map(|_| ());
    let irc_to_discord = irc_send_recv
        .map_err(|_| format_err!("IRC hung up?"))
        .map(|(chan, sender, msg)| {
            iter_ok(Config::discord_for_irc(chan))
                .map(move |chan| (chan, sender.clone(), msg.clone()))
        })
        .flatten()
        .forward(discord_recv_send.sink_map_err(|_| format_err!("Can't send to Discord")))
        .map(|_| ());

    discord_side
        .join4(irc_side, discord_to_irc, irc_to_discord)
        .map(|((), (), (), ())| ())
}
