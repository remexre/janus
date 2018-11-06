use failure::{format_err, Error, SyncFailure};
use futures::{
    future::{err, Either, Future},
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot::channel,
    },
    Stream,
};
use log::error;
use serenity::{
    client::{Client, Context, EventHandler},
    model::{channel::Message, id::ChannelId},
};
use std::thread::spawn;

/// Starts listening for Discord messages, communicating over the given channels.
pub fn start_discord(
    discord_token: &str,
    discord_send: UnboundedSender<(u64, String, String)>,
    discord_recv: UnboundedReceiver<(u64, String, String)>,
) -> impl Future<Item = (), Error = Error> {
    match Client::new(discord_token, Handler(discord_send)) {
        Ok(mut discord) => {
            let (end_send, end_recv) = channel();
            spawn(move || {
                let err = loop {
                    match discord.start() {
                        Ok(()) => {}
                        Err(e) => break Error::from(SyncFailure::new(e)),
                    }
                };
                let _ = end_send.send(err);
            });

            Either::A(
                discord_recv
                    .map_err(|()| unreachable!())
                    .for_each(|(chan, sender, msg)| {
                        match ChannelId(chan).say(format!("{}: {}", sender, msg)) {
                            Ok(_) => {}
                            Err(err) => {
                                error!("{}", err);
                            }
                        }
                        Ok(())
                    })
                    .and_then(|()| {
                        end_recv
                            .map_err(|_| format_err!("Discord client thread panicked!"))
                            .and_then(err)
                    }),
            )
        }
        Err(e) => Either::B(err(Error::from(SyncFailure::new(e)))),
    }
}

struct Handler(UnboundedSender<(u64, String, String)>);

impl EventHandler for Handler {
    fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        if let Err(err) = self
            .0
            .unbounded_send((msg.channel_id.0, msg.author.name, msg.content))
        {
            error!("{}", err);
            ctx.quit();
        }
    }
}
