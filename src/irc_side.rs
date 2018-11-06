use failure::{format_err, Error};
use futures::{
    future::{err, Either, Future},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    Stream,
};
use irc::{
    client::{data::config::Config, ext::ClientExt, Client, IrcClient},
    proto::command::Command,
};

/// Starts listening for IRC messages, communicating over the given channels.
pub fn start_irc(
    config: Config,
    irc_send: UnboundedSender<(String, String, String)>,
    irc_recv: UnboundedReceiver<(String, String, String)>,
) -> impl Future<Item = (), Error = Error> {
    match IrcClient::from_config(config) {
        Ok(client) => {
            if let Err(e) = client.identify() {
                return Either::B(err(Error::from(e)));
            }

            let send_client = client.clone();

            let recv_fut = client.stream().map_err(Error::from).for_each(move |msg| {
                match (msg.source_nickname(), &msg.command) {
                    (Some(sender), Command::PRIVMSG(chan, msg)) => irc_send
                        .unbounded_send((chan.to_string(), sender.to_string(), msg.to_string()))
                        .map_err(|_| format_err!("Couldn't send an IRC message")),
                    _ => Ok(()),
                }
            });
            let send_fut =
                irc_recv
                    .map_err(|()| unreachable!())
                    .for_each(move |(chan, sender, msg)| {
                        send_client
                            .send_privmsg(chan, format!("{}: {}", sender, msg))
                            .map_err(Error::from)
                    });
            Either::A(recv_fut.join(send_fut).map(|((), ())| ()))
        }
        Err(e) => Either::B(err(Error::from(e))),
    }
}
