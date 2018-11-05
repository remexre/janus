mod util;

use failure::Fallible;
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
    unimplemented!()
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
