use automata::TransitionSystem;
use automata::hoa::WriteHoa;
use tracing::{debug, info, trace};
use tracing_subscriber::{filter, prelude::*};

use automata::alphabet::{CharAlphabet, PropAlphabet};
use automata::automaton::DeterministicOmegaAutomaton;
use clap::{Arg, ArgMatches, Command};

fn cli() -> clap::Command {
    Command::new("oai")
    .about("Omega automata interaction")
    .subcommand_required(true)
    .arg(
        Arg::new("verbosity")
        .short('v')
        .long("verbosity")
        .num_args(0..=1)
        .require_equals(true)
        .value_parser(["info", "debug", "trace"])
        .default_missing_value("info")
    )
    .subcommand(
        Command::new("todpa")
        .about("reads HOA automaton from stdin and tries to convert it into a deterministic parity automaton")
    )
}

fn setup_logging(matches: &ArgMatches) {
    let level = match matches
        .try_get_one::<String>("verbosity")
        .ok()
        .flatten()
        .map(|m| m.as_str())
    {
        Some("trace") => filter::LevelFilter::TRACE,
        Some("debug") => filter::LevelFilter::DEBUG,
        Some("info") => filter::LevelFilter::INFO,
        _ => filter::LevelFilter::INFO,
    };

    let stdout_log = tracing_subscriber::fmt::layer()
        .pretty()
        .with_writer(std::io::stderr);

    tracing_subscriber::registry()
        .with(stdout_log.with_filter(level))
        .init();

    trace!("setup {level} logging");
}

pub fn main() {
    let matches = cli().get_matches();

    setup_logging(&matches);

    debug!("reading automata from stdin");
    let stream = automata::hoa::IntoDeterministicHoaAutomatonStream::new(std::io::stdin().lock());

    match matches.subcommand() {
        Some(("todpa", _sub_matches)) => {
            debug!("converting input automata into DPAs");

            for aut in stream {
                debug!("read deterministic automaton with {} states", aut.size());

                let start = std::time::Instant::now();
                let converted: DeterministicOmegaAutomaton<CharAlphabet> = aut.into();
                info!(
                    "conversion into char alphabet automaton took {}µs",
                    start.elapsed().as_micros()
                );

                let start = std::time::Instant::now();
                let reconverted: DeterministicOmegaAutomaton<PropAlphabet> =
                    converted.try_into().unwrap();
                info!(
                    "conversion into HOA alphabet automaton took {}µs",
                    start.elapsed().as_micros()
                );

                match reconverted.into_dpa() {
                    Ok(dpa) => print!("{}", dpa.to_hoa()),
                    Err(error) => {
                        eprintln!("error: {error}");
                        std::process::exit(1);
                    }
                }
            }
        }
        _ => unreachable!(),
    }
}
