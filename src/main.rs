#[macro_use]
extern crate serde_derive;

mod commands;
mod config;
mod discord_side;
mod irc_side;
#[cfg(feature = "signals")]
mod signals;
mod util;

use failure::{Error, Fallible};
use futures::future::{Either, Future};
use std::{collections::HashSet, path::PathBuf};
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
    config::Config::init(&opts.config_file)?;

    let fut = match opts.subcommand {
        Subcommand::ListChannels { as_bindings } => Either::A(
            commands::list_channels(&opts.discord_token).and_then(move |chans| {
                if as_bindings {
                    use crate::config::Binding;

                    #[derive(Serialize)]
                    struct Wrapper {
                        bindings: Vec<Binding>,
                    }

                    let irc_channels = chans.irc.into_iter().collect::<HashSet<String>>();
                    let bindings = chans
                        .discord
                        .values()
                        .flatten()
                        .filter_map(|&(ref name, id)| {
                            let irc_name = format!("#{}", name);
                            if irc_channels.contains(&irc_name) {
                                Some(Binding {
                                    irc: irc_name,
                                    discord: id,
                                })
                            } else {
                                None
                            }
                        })
                        .collect();
                    let bindings = toml::to_string_pretty(&Wrapper { bindings })?;
                    println!("{}", bindings);
                    Ok(())
                } else {
                    serde_json::to_writer_pretty(std::io::stdout(), &chans).map_err(Error::from)
                }
            }),
        ),
        Subcommand::Run => Either::B(commands::run(&opts.discord_token)),
    };
    // TODO: Use a real runtime.
    fut.wait()
}

#[derive(StructOpt)]
#[structopt(raw(setting = "::structopt::clap::AppSettings::ColoredHelp"))]
struct Options {
    /// Turns off message output. Passing once prevents logging to syslog. Passing twice or more
    /// disables all logging.
    #[structopt(short = "q", long = "quiet", parse(from_occurrences))]
    pub quiet: usize,

    /// Increases the verbosity. Default verbosity is warnings and higher to syslog, info and
    /// higher to the console.
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,

    /// The Discord bot token.
    #[structopt(env = "DISCORD_TOKEN")]
    pub discord_token: String,

    /// The syslog server to send logs to.
    #[structopt(short = "s", long = "syslog-server", env = "SYSLOG_SERVER")]
    pub syslog_server: Option<String>,

    /// The config file to read.
    #[structopt(
        short = "c",
        long = "config-file",
        default_value = "janus.toml",
        env = "CONFIG_FILE",
        parse(from_os_str)
    )]
    config_file: PathBuf,

    /// The subcommand to run.
    #[structopt(subcommand)]
    pub subcommand: Subcommand,
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

#[derive(StructOpt)]
enum Subcommand {
    /// Lists the channels available.
    #[structopt(name = "list")]
    ListChannels {
        /// Whether to output in the form of bindings sections or not.
        #[structopt(long = "as-bindings")]
        as_bindings: bool,
    },

    /// Starts Janus.
    #[structopt(name = "run")]
    Run,
}
