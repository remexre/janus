use crate::config::Config;
use failure::{format_err, Error, Fallible};
use futures::{
    future::{err, Either, Future},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    Stream,
};
use irc::{
    client::{data::config::Config as IrcConfig, ext::ClientExt, Client, IrcClient},
    proto::{command::Command, response::Response},
};
use std::collections::HashSet;

/// Starts listening for IRC messages, communicating over the given channels.
pub fn start_irc(
    config: IrcConfig,
    irc_send: UnboundedSender<(String, String, String)>,
    irc_recv: UnboundedReceiver<(String, String, String)>,
) -> impl Future<Item = (), Error = Error> {
    match IrcClient::from_config(config) {
        Ok(client) => {
            if let Err(e) = client.identify() {
                return Either::B(err(Error::from(e)));
            }

            let recv_client = client.clone();
            let recv_fut = client.stream().map_err(Error::from).for_each(move |msg| {
                match (msg.source_nickname(), &msg.command) {
                    (Some(sender), Command::PRIVMSG(chan, msg)) => irc_send
                        .unbounded_send((chan.to_string(), sender.to_string(), msg.to_string()))
                        .map_err(|_| format_err!("Couldn't send an IRC message")),
                    (_, Command::Response(Response::RPL_ENDOFMOTD, _, _)) => {
                        ensure_joined(&recv_client)
                    }
                    _ => Ok(()),
                }
            });
            let send_client = client.clone();
            let send_fut =
                irc_recv
                    .map_err(|()| unreachable!())
                    .for_each(move |(chan, sender, msg)| {
                        send_client
                            .send_privmsg(chan, format!("{}: {}", sender, msg))
                            .map_err(Error::from)
                    });
            let update = Config::notify_on_reload()
                .map_err(|()| unreachable!())
                .for_each(move |()| ensure_joined(&client));
            Either::A(update.join3(recv_fut, send_fut).map(|((), (), ())| ()))
        }
        Err(e) => Either::B(err(Error::from(e))),
    }
}

fn ensure_joined(client: &impl ClientExt) -> Fallible<()> {
    let current_channels: HashSet<String> = client
        .list_channels()
        .map(|chans| chans.into_iter().collect())
        .unwrap_or_default();
    let chans_to_join = Config::irc_channels()
        .into_iter()
        .filter(|chan| !current_channels.contains(chan));

    for chan in chans_to_join {
        client.send_join(chan)?;
    }
    Ok(())
}
