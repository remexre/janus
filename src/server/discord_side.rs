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
    model::{
        channel::Message,
        gateway::Ready,
        id::{ChannelId, UserId},
    },
};
use std::{
    iter::once,
    sync::{Arc, RwLock},
    thread::spawn,
};

/// Starts listening for Discord messages, communicating over the given channels.
pub fn start_discord(
    discord_token: &str,
    discord_send: UnboundedSender<(u64, String, String)>,
    discord_recv: UnboundedReceiver<(u64, Arc<String>)>,
) -> impl Future<Item = (), Error = Error> {
    match Client::new(discord_token, Handler(discord_send, RwLock::new(UserId(0)))) {
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
                    .for_each(|(chan, msg)| {
                        match ChannelId(chan).say(msg) {
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

struct Handler(UnboundedSender<(u64, String, String)>, RwLock<UserId>);

impl EventHandler for Handler {
    fn ready(&self, _ctx: Context, ready: Ready) {
        *self.1.write().unwrap() = ready.user.id;
    }

    fn message(&self, ctx: Context, msg: Message) {
        if msg.author.id == *self.1.read().unwrap() {
            return;
        }

        let chan_id = msg.channel_id.0;
        let author = msg.author.name;
        let iter = once((chan_id, author.clone(), msg.content)).chain(
            msg.attachments
                .iter()
                .map(|a| (chan_id, author.clone(), a.url.clone())),
        );
        for data in iter {
            if let Err(err) = self.0.unbounded_send(data) {
                error!("{}", err);
                ctx.quit();
                break;
            }
        }
    }
}
