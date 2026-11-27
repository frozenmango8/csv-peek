use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use csv_peek::{Config, Error};

#[derive(Parser)]
#[command(version, about = "Preview CSV files from the terminal")]
struct Args {
    /// CSV file to read. Omit, or pass -, to read stdin.
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// How many data rows to show.
    #[arg(short = 'n', long, default_value_t = 10, value_name = "N")]
    rows: usize,

    /// Field delimiter. One ASCII character, or `\t`. Sniffed if omitted.
    #[arg(short, long, value_name = "CHAR", value_parser = parse_delimiter)]
    delimiter: Option<u8>,

    /// Treat the first row as data, not a header.
    #[arg(long)]
    no_header: bool,
}

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // `csv-peek file | head` closes stdout early. That is not a failure.
            if e.is_broken_pipe() {
                return ExitCode::SUCCESS;
            }
            eprintln!("csv-peek: {e}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<(), Error> {
    let args = Args::parse();
    let cfg = Config {
        rows: args.rows,
        delimiter: args.delimiter,
        has_header: !args.no_header,
    };
    let input = open_input(args.file)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    csv_peek::run(input, &mut out, &cfg)
}

fn open_input(path: Option<PathBuf>) -> Result<Box<dyn Read>, Error> {
    match path {
        None => Ok(Box::new(io::stdin())),
        Some(p) if p.as_os_str() == "-" => Ok(Box::new(io::stdin())),
        Some(p) => match File::open(&p) {
            Ok(file) => Ok(Box::new(file)),
            Err(e) => Err(Error::from(io::Error::new(
                e.kind(),
                format!("{}: {e}", p.display()),
            ))),
        },
    }
}

fn parse_delimiter(raw: &str) -> Result<u8, String> {
    let ch = match raw {
        r"\t" | "tab" => '\t',
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c,
                _ => return Err("delimiter must be a single character".into()),
            }
        }
    };
    if ch == '\t' || (ch.is_ascii() && !ch.is_control()) {
        Ok(ch as u8)
    } else {
        Err("delimiter must be a single ASCII character".into())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_delimiter;

    #[test]
    fn parses_common_delimiters() {
        assert_eq!(parse_delimiter(",").unwrap(), b',');
        assert_eq!(parse_delimiter(";").unwrap(), b';');
        assert_eq!(parse_delimiter("|").unwrap(), b'|');
        assert_eq!(parse_delimiter(r"\t").unwrap(), b'\t');
        assert_eq!(parse_delimiter("tab").unwrap(), b'\t');
        assert_eq!(parse_delimiter("\t").unwrap(), b'\t');
    }

    #[test]
    fn rejects_multi_character_and_controls() {
        assert!(parse_delimiter(",,").is_err());
        assert!(parse_delimiter("").is_err());
        assert!(parse_delimiter("\n").is_err());
        assert!(parse_delimiter("λ").is_err());
    }
}
