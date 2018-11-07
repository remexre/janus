use crate::config::Config;
use failure::{format_err, Error, SyncFailure};
use futures::{
    future::{err, Either, Future},
    sync::oneshot::{channel, Sender},
    Stream,
};
use irc::{
    client::{ext::ClientExt, Client as IrcClientTrait, IrcClient},
    proto::{command::Command, response::Response},
};
use serenity::{
    client::{Client, Context, EventHandler},
    model::{
        channel::ChannelType,
        gateway::Ready,
        guild::{Guild, GuildStatus},
        id::GuildId,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    thread::spawn,
};

/// A listing of all available channels.
#[derive(Debug, Serialize)]
pub struct Channels {
    /// The available IRC channels.
    pub irc: Vec<String>,

    /// A mapping from available Discord servers to their channels.
    pub discord: HashMap<String, Vec<(String, u64)>>,
}

pub fn run(discord_token: &str) -> impl Future<Item = Channels, Error = Error> {
    get_irc_channels()
        .join(get_discord_channels(discord_token))
        .map(|(irc, discord)| Channels { irc, discord })
}

fn get_irc_channels() -> impl Future<Item = Vec<String>, Error = Error> {
    match IrcClient::from_config(Config::irc_config()) {
        Ok(client) => {
            if let Err(e) = client.identify() {
                return Either::B(err(Error::from(e)));
            }

            let mut list = Some(Vec::new());
            Either::A(
                client
                    .stream()
                    .map_err(Error::from)
                    .and_then(move |msg| match msg.command {
                        Command::Response(Response::RPL_LIST, args, _) => {
                            list.as_mut().unwrap().push(args[1].clone());
                            Ok(None)
                        }
                        Command::Response(Response::RPL_LISTEND, _, _) => {
                            Ok(Some(list.take().unwrap()))
                        }
                        Command::Response(Response::RPL_ENDOFMOTD, _, _) => {
                            match client.send(Command::LIST(None, None)) {
                                Ok(()) => Ok(None),
                                Err(e) => Err(Error::from(e)),
                            }
                        }
                        _ => Ok(None),
                    })
                    .filter_map(|o| o)
                    .into_future()
                    .map_err(|(e, _)| e)
                    .and_then(|(o, _)| match o {
                        Some(o) => Ok(o),
                        None => Err(format_err!("IRC client stopped unexpectedly")),
                    }),
            )
        }
        Err(e) => Either::B(err(Error::from(e))),
    }
}

fn get_discord_channels(
    discord_token: &str,
) -> impl Future<Item = HashMap<String, Vec<(String, u64)>>, Error = Error> {
    let (send, recv) = channel();
    match Client::new(
        discord_token,
        DiscordHandler(
            Mutex::new(Some(send)),
            Mutex::new(Some(HashMap::new())),
            Mutex::new(HashSet::new()),
        ),
    ) {
        Ok(mut discord) => {
            spawn(move || discord.start().ok());
            Either::A(recv.map_err(|e| format_err!("Serenity panicked: {}", e)))
        }
        Err(e) => Either::B(err(Error::from(SyncFailure::new(e)))),
    }
}

struct DiscordHandler(
    Mutex<Option<Sender<HashMap<String, Vec<(String, u64)>>>>>,
    Mutex<Option<HashMap<String, Vec<(String, u64)>>>>,
    Mutex<HashSet<GuildId>>,
);

impl DiscordHandler {
    fn store_guild(&self, ctx: &Context, guild: Guild) {
        let mut ids = self.2.lock().unwrap();
        let mut chan_list = self.1.lock().unwrap();

        if !ids.contains(&guild.id) {
            return;
        }

        let chans = guild
            .channels
            .values()
            .filter(|chan| chan.read().kind == ChannelType::Text)
            .map(|chan| {
                let chan = chan.read();
                (chan.name.clone(), chan.id.0)
            })
            .collect();
        chan_list.as_mut().unwrap().insert(guild.name, chans);

        ids.remove(&guild.id);
        if ids.is_empty() {
            self.0
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .send(chan_list.take().unwrap())
                .ok();
            ctx.quit();
        }
    }
}

impl EventHandler for DiscordHandler {
    fn ready(&self, ctx: Context, data: Ready) {
        self.2
            .lock()
            .unwrap()
            .extend(data.guilds.iter().map(|g| match g {
                GuildStatus::OnlinePartialGuild(g) => g.id,
                GuildStatus::OnlineGuild(g) => g.id,
                GuildStatus::Offline(g) => g.id,
            }));
        for guild in data.guilds {
            match guild {
                GuildStatus::OnlineGuild(g) => self.store_guild(&ctx, g),
                _ => {}
            }
        }
    }

    fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
        self.store_guild(&ctx, guild);
    }
}
