use failure::Fallible;
use futures::sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use irc::client::data::config::Config as IrcConfig;
use lazy_static::lazy_static;
use log::warn;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

lazy_static! {
    static ref CONFIG: Arc<RwLock<Config>> = Arc::new(RwLock::new(Config {
        irc: IrcConfig::default(),
        bindings: Vec::new(),
    }));
    static ref NOTIFY_MES: Arc<Mutex<Vec<UnboundedSender<()>>>> = Arc::new(Mutex::new(Vec::new()));
}

/// The configuration for a bridge between a Discord server and an IRC server.
#[derive(Deserialize)]
pub struct Config {
    /// IRC configuration.
    irc: IrcConfig,

    /// Bindings between two channels.
    bindings: Vec<Binding>,
}

impl Config {
    /// Returns the Discord channels that should be sent messages from the named IRC channel.
    pub fn discord_for_irc(irc: String) -> Vec<u64> {
        CONFIG
            .read()
            .unwrap()
            .bindings
            .iter()
            .filter(move |b| b.irc == irc)
            .map(|b| b.discord)
            .collect()
    }

    /// Initializes the config to the one at the given path.
    pub fn init(path: PathBuf) -> Fallible<()> {
        Config::reload_from(&path)?;

        #[cfg(feature = "signals")]
        {
            let (send, recv) = std::sync::mpsc::channel();
            unsafe { crate::signals::add_sighup_handler(send)? };
            std::thread::spawn(move || {
                while let Ok(()) = recv.recv() {
                    warn!("Reloading config...");
                    if let Err(e) = Config::reload_from(&path) {
                        crate::util::log_err(e)
                    }
                }
            });
        }

        Ok(())
    }

    /// Returns the channels that should be connected to.
    pub fn irc_channels() -> Vec<String> {
        CONFIG
            .read()
            .unwrap()
            .bindings
            .iter()
            .map(|b| b.irc.clone())
            .collect()
    }

    /// Returns the IRC config.
    pub fn irc_config() -> IrcConfig {
        CONFIG.read().unwrap().irc.clone()
    }

    /// Returns the IRC channels that should be sent messages from the named Discord channel.
    pub fn irc_for_discord(discord: u64) -> Vec<String> {
        CONFIG
            .read()
            .unwrap()
            .bindings
            .iter()
            .filter(move |b| b.discord == discord)
            .map(|b| b.irc.clone())
            .collect()
    }

    /// Loads the config from a file.
    fn load_from(path: impl AsRef<Path>) -> Fallible<Config> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let config = toml::from_slice(&data)?;
        Ok(config)
    }

    /// Returns a channel that will be sent `()` on config reloads.
    pub fn notify_on_reload() -> UnboundedReceiver<()> {
        let (send, recv) = unbounded();
        NOTIFY_MES.lock().unwrap().push(send);
        recv
    }

    /// Reloads the config from a file.
    pub fn reload_from(path: impl AsRef<Path>) -> Fallible<()> {
        Config::load_from(path).map(|config| {
            *CONFIG.write().unwrap() = config;

            // TODO: It ought to be possible to make this more efficient without running afoul of
            // Sync...
            let mut notify_mes = NOTIFY_MES.lock().unwrap();
            let new_notifies = notify_mes
                .drain(..)
                .filter_map(|chan| {
                    if chan.unbounded_send(()).is_ok() {
                        Some(chan)
                    } else {
                        None
                    }
                })
                .collect();
            *notify_mes = new_notifies;
        })
    }
}

#[derive(Deserialize, Serialize)]
pub struct Binding {
    /// The Discord channel ID.
    pub discord: u64,

    /// The IRC channel name.
    pub irc: String,
}
