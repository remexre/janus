use failure::Error;
use futures::{
    future::{err, ok, Either, Future},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    Stream,
};
use irc::client::{data::config::Config, ext::ClientExt, Client, IrcClient};

/// Starts listening for IRC messages, communicating over the given channels.
pub fn start_irc(
    config: Config,
    irc_send: UnboundedSender<(String, String, String)>,
    irc_recv: UnboundedReceiver<(String, String, String)>,
) -> impl Future<Item = (), Error = Error> {
    match IrcClient::from_config(config) {
        Ok(client) => {
            let recv_fut = client.stream().map_err(Error::from).for_each(|msg| {
                println!("{:#?}", msg);
                ok(())
            });
            let send_fut = irc_recv
                .map_err(|()| unreachable!())
                .for_each(|(chan, sender, msg)| {
                    unimplemented!();
                    ok(())
                });
            Either::A(recv_fut.join(send_fut).map(|((), ())| ()))
        }
        Err(e) => Either::B(err(Error::from(e))),
    }
}
