use std::fs::File;
use std::io::{self, IsTerminal, Read};
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

    /// Output width in columns. Defaults to the terminal, or 80 if stdout is not one.
    #[arg(short = 'w', long, value_name = "COLS")]
    width: Option<usize>,

    /// Comma-separated column names or 1-based indexes.
    #[arg(long, value_name = "LIST")]
    cols: Option<String>,

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
        width: resolve_width(args.width),
        columns: args.cols.as_deref().map(parse_cols).transpose()?,
    };
    let input = open_input(args.file)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    csv_peek::run(input, &mut out, &cfg)
}

fn parse_cols(raw: &str) -> Result<Vec<String>, Error> {
    let cols: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if cols.is_empty() {
        return Err(Error::new("no columns selected"));
    }
    Ok(cols)
}

fn resolve_width(flag: Option<usize>) -> usize {
    if let Some(w) = flag {
        return w.max(1);
    }
    if io::stdout().is_terminal() {
        if let Some((terminal_size::Width(w), _)) = terminal_size::terminal_size() {
            if w > 0 {
                return usize::from(w);
            }
        }
    }
    80
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
    use super::{parse_cols, parse_delimiter};

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
    fn parses_a_column_list() {
        assert_eq!(parse_cols("a, b").unwrap(), vec!["a", "b"]);
        assert!(parse_cols(" , ").is_err());
    }

    #[test]
    fn rejects_multi_character_and_controls() {
        assert!(parse_delimiter(",,").is_err());
        assert!(parse_delimiter("").is_err());
        assert!(parse_delimiter("\n").is_err());
        assert!(parse_delimiter("λ").is_err());
    }
}
