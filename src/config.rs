use failure::Fallible;
use irc::client::data::config::Config as IrcConfig;
use lazy_static::lazy_static;
use std::{
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, RwLock},
};

lazy_static! {
    static ref CONFIG: Arc<RwLock<Config>> = Arc::new(RwLock::new(Config {
        irc: IrcConfig::default(),
        bindings: Vec::new(),
    }));
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
    pub fn init(path: impl AsRef<Path>) -> Fallible<()> {
        Config::reload_from(path)?;
        // TODO: Set up SIGHUP handler.
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

    /// Reloads the config from a file.
    pub fn reload_from(path: impl AsRef<Path>) -> Fallible<()> {
        Config::load_from(path).map(|config| {
            *CONFIG.write().unwrap() = config;
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
