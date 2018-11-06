#[macro_use]
extern crate serde_derive;

mod discord_side;
mod irc_side;
mod util;

use failure::{format_err, Error, Fallible};
use futures::{future::Future, sync::mpsc::unbounded, Sink, Stream};
use irc::client::{data::config::Config as IrcConfig, IrcClient};
use std::{fs::File, io::Read, path::PathBuf};
use structopt::StructOpt;

fn main() {
    dotenv::dotenv().ok();
    let opts = Options::from_args();
    if let Err(err) = run(opts) {
        util::log_err(err);
        std::process::exit(1);
    }
}

fn run(opts: Options) -> Fallible<()> {
    opts.start_logger()?;
    let config: Config = {
        let mut file = File::open(opts.config_file)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        toml::from_slice(&data)?
    };

    let (discord_send, discord_send_recv) = unbounded();
    let (discord_recv_send, discord_recv) = unbounded();
    let (irc_send, irc_send_recv) = unbounded();
    let (irc_recv_send, irc_recv) = unbounded();

    //let discord_side = discord_side::start_discord(&opts.discord_token, discord_send, discord_recv);
    let irc_side = irc_side::start_irc(config.irc.clone(), irc_send, irc_recv);
    let discord_to_irc = discord_send_recv
        .map_err(|_| format_err!("Discord hung up?"))
        .map(|(chan, sender, msg): (u64, String, String)| {
            config
                .irc_for_discord(chan)
                .map(move |chan| (chan, sender.clone(), msg.clone()))
        })
        .flatten()
        .forward(irc_recv_send.sink_map_err(|_| format_err!("Can't send to IRC")))
        .map(|_| ());
    let irc_to_discord = irc_send_recv
        .map_err(|_| format_err!("IRC hung up?"))
        .map(|(chan, sender, msg)| {
            config
                .discord_for_irc(chan)
                .map(move |chan| (chan, sender.clone(), msg.clone()))
        })
        .flatten()
        .forward(discord_recv_send.sink_map_err(|_| format_err!("Can't send to Discord")))
        .map(|_| ());

    irc_side
        .join3(discord_to_irc, irc_to_discord)
        .wait()
        .map(|((), (), ())| ())
}

/// The configuration for a bridge between a Discord server and an IRC server.
#[derive(Deserialize)]
pub struct Config {
    /// IRC configuration.
    pub irc: IrcConfig,
}

impl Config {
    /// Returns the Discord channels that should be sent messages from the named IRC channel.
    pub fn discord_for_irc(&self, irc: String) -> impl Stream<Item = u64, Error = Error> {
        unimplemented!();
        futures::stream::empty()
    }

    /// Returns the IRC channels that should be sent messages from the named Discord channel.
    pub fn irc_for_discord(&self, discord: u64) -> impl Stream<Item = String, Error = Error> {
        unimplemented!();
        futures::stream::empty()
    }
}

#[derive(StructOpt)]
#[structopt(raw(setting = "::structopt::clap::AppSettings::ColoredHelp"))]
struct Options {
    /// Turns off message output. Passing once prevents logging to syslog. Passing twice or more
    /// disables all logging.
    #[structopt(short = "q", long = "quiet", parse(from_occurrences))]
    quiet: usize,

    /// Increases the verbosity. Default verbosity is warnings and higher to syslog, info and
    /// higher to the console.
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    /// The config file to read.
    #[structopt(
        short = "c",
        long = "config-file",
        default_value = "janus.toml",
        env = "CONFIG_FILE",
        parse(from_os_str)
    )]
    config_file: PathBuf,

    /// The Discord bot token.
    #[structopt(env = "DISCORD_TOKEN")]
    discord_token: String,

    /// The syslog server to send logs to.
    #[structopt(short = "s", long = "syslog-server", env = "SYSLOG_SERVER")]
    syslog_server: Option<String>,
}

impl Options {
    /// Sets up logging as specified by the `-q`, `-s`, and `-v` flags.
    fn start_logger(&self) -> Fallible<()> {
        use fern::Dispatch;
        use log::LevelFilter;

        if self.quiet >= 2 {
            return Ok(());
        }

        let (console_ll, syslog_ll) = match self.verbose {
            0 => (LevelFilter::Info, LevelFilter::Warn),
            1 => (LevelFilter::Debug, LevelFilter::Info),
            2 => (LevelFilter::Trace, LevelFilter::Debug),
            _ => (LevelFilter::Trace, LevelFilter::Trace),
        };

        let fern = Dispatch::new().chain(
            Dispatch::new()
                .level(console_ll)
                .format(move |out, message, record| {
                    out.finish(format_args!("[{}] {}", record.level(), message))
                })
                .chain(std::io::stderr()),
        );

        let fern = if self.quiet == 0 {
            let formatter = syslog::Formatter3164 {
                facility: syslog::Facility::LOG_DAEMON,
                hostname: hostname::get_hostname(),
                process: "thetis".to_owned(),
                pid: ::std::process::id() as i32,
            };

            let syslog = if let Some(ref server) = self.syslog_server {
                syslog::tcp(formatter, server).map_err(failure::SyncFailure::new)?
            } else {
                syslog::unix(formatter.clone())
                    .or_else(|_| syslog::tcp(formatter.clone(), ("127.0.0.1", 601)))
                    .or_else(|_| {
                        syslog::udp(formatter.clone(), ("127.0.0.1", 0), ("127.0.0.1", 514))
                    })
                    .map_err(failure::SyncFailure::new)?
            };

            fern.chain(Dispatch::new().level(syslog_ll).chain(syslog))
        } else {
            fern
        };

        fern.apply()?;
        Ok(())
    }
}
