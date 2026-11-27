use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Read, Write};

mod sniff;
mod table;

const SAMPLE: usize = 64 * 1024;

type Rewound<R> = io::Chain<io::Cursor<Vec<u8>>, R>;

#[derive(Debug, Clone)]
pub struct Config {
    pub rows: usize,
    pub delimiter: Option<u8>,
    pub has_header: bool,
    pub width: usize,
    pub columns: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rows: 10,
            delimiter: None,
            has_header: true,
            width: 80,
            columns: None,
        }
    }
}

#[derive(Debug)]
pub struct Error {
    message: String,
    pipe: bool,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            pipe: false,
        }
    }

    pub fn is_broken_pipe(&self) -> bool {
        self.pipe
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self {
            pipe: err.kind() == io::ErrorKind::BrokenPipe,
            message: err.to_string(),
        }
    }
}

impl From<csv::Error> for Error {
    fn from(err: csv::Error) -> Self {
        let pipe = match err.kind() {
            csv::ErrorKind::Io(io_err) => io_err.kind() == io::ErrorKind::BrokenPipe,
            _ => false,
        };
        Self {
            message: err.to_string(),
            pipe,
        }
    }
}

pub fn run<R: Read, W: Write>(input: R, output: &mut W, cfg: &Config) -> Result<(), Error> {
    let mut sheet = open(input, cfg.delimiter, cfg.has_header)?;
    if sheet.headers.is_empty() {
        return Ok(());
    }
    let idxs = match &cfg.columns {
        Some(spec) => select_columns(&sheet.headers, spec)?,
        None => (0..sheet.headers.len()).collect(),
    };
    let header = project_names(&sheet.headers, &idxs);
    let rows = collect_rows(&mut sheet, &idxs, cfg.rows)?;
    let rendered = table::render(&header, &rows, cfg.width);
    output.write_all(rendered.as_bytes())?;
    Ok(())
}

struct Sheet<R: Read> {
    reader: csv::Reader<Rewound<R>>,
    headers: Vec<String>,
    pending: Option<csv::StringRecord>,
}

fn open<R: Read>(src: R, delimiter: Option<u8>, has_header: bool) -> Result<Sheet<R>, Error> {
    let (delim, chained) = buffer_sample(src, delimiter)?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(has_header)
        .from_reader(chained);
    let (headers, pending) = read_headers(&mut reader, has_header)?;
    Ok(Sheet {
        reader,
        headers,
        pending,
    })
}

fn buffer_sample<R: Read>(mut src: R, delimiter: Option<u8>) -> Result<(u8, Rewound<R>), Error> {
    let mut buf = Vec::new();
    (&mut src).take(SAMPLE as u64).read_to_end(&mut buf)?;
    sniff::strip_bom(&mut buf);
    let delim = match delimiter {
        Some(d) => d,
        None => sniff::sniff_delimiter(sniff::prefix(&buf, SAMPLE)),
    };
    Ok((delim, io::Cursor::new(buf).chain(src)))
}

fn read_headers<R: Read>(
    reader: &mut csv::Reader<R>,
    has_header: bool,
) -> Result<(Vec<String>, Option<csv::StringRecord>), Error> {
    if has_header {
        let names = reader.headers()?.iter().map(str::to_string).collect();
        return Ok((normalize_headers(names), None));
    }
    let mut rec = csv::StringRecord::new();
    if !reader.read_record(&mut rec)? {
        return Ok((Vec::new(), None));
    }
    let names = (1..=rec.len()).map(|i| i.to_string()).collect();
    Ok((names, Some(rec)))
}

fn normalize_headers(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            if name.trim().is_empty() {
                (i + 1).to_string()
            } else {
                name
            }
        })
        .collect()
}

fn select_columns(headers: &[String], spec: &[String]) -> Result<Vec<usize>, Error> {
    let mut idxs = Vec::with_capacity(spec.len());
    for col in spec {
        if let Some(i) = headers.iter().position(|h| h == col) {
            idxs.push(i);
            continue;
        }
        if let Ok(n) = col.parse::<usize>() {
            if n > 0 && n <= headers.len() {
                idxs.push(n - 1);
                continue;
            }
        }
        return Err(Error::new(format!("column not found: {col}")));
    }
    Ok(idxs)
}

fn project_names(headers: &[String], idxs: &[usize]) -> Vec<String> {
    idxs.iter().map(|&i| headers[i].clone()).collect()
}

fn project(rec: &csv::StringRecord, idxs: &[usize]) -> Vec<String> {
    idxs.iter()
        .map(|&i| rec.get(i).unwrap_or("").to_string())
        .collect()
}

fn collect_rows<R: Read>(
    sheet: &mut Sheet<R>,
    idxs: &[usize],
    limit: usize,
) -> Result<Vec<Vec<String>>, Error> {
    let mut rows = Vec::new();
    if let Some(rec) = sheet.pending.take() {
        if limit > 0 {
            rows.push(project(&rec, idxs));
        }
    }
    let mut rec = csv::StringRecord::new();
    while rows.len() < limit && sheet.reader.read_record(&mut rec)? {
        rows.push(project(&rec, idxs));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::Read;
    use std::rc::Rc;

    use super::*;

    fn preview(data: &[u8], cfg: Config) -> String {
        let mut out = Vec::new();
        run(data, &mut out, &cfg).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn reads_a_header_and_rows() {
        let out = preview(b"name,age\nann,3\nbob,4\n", Config::default());
        assert_eq!(out, "name  age\nann   3\nbob   4\n");
    }

    #[test]
    fn sniffs_semicolon_and_tab() {
        let semi = preview(b"name;age\nann;3\n", Config::default());
        assert_eq!(semi, "name  age\nann   3\n");
        let tab = preview(b"name\tage\nann\t3\n", Config::default());
        assert_eq!(tab, "name  age\nann   3\n");
    }

    #[test]
    fn explicit_delimiter_wins() {
        let cfg = Config {
            delimiter: Some(b'|'),
            ..Config::default()
        };
        let out = preview(b"name|age\nann|3\n", cfg);
        assert_eq!(out, "name  age\nann   3\n");
    }

    #[test]
    fn strips_a_utf8_bom() {
        let data = b"\xEF\xBB\xBFname,age\nann,3\n";
        assert_eq!(preview(data, Config::default()), "name  age\nann   3\n");
    }

    #[test]
    fn quoted_commas_stay_in_the_field() {
        let data = b"name,note\nann,\"hello, world\"\n";
        let out = preview(data, Config::default());
        assert_eq!(out, "name  note\nann   hello, world\n");
    }

    #[test]
    fn newlines_inside_quotes_do_not_break_the_row() {
        let data = b"name,note\nann,\"hello\nworld\"\n";
        let out = preview(data, Config::default());
        assert_eq!(out, "name  note\nann   hello world\n");
    }

    #[test]
    fn respects_the_row_limit() {
        let cfg = Config {
            rows: 1,
            ..Config::default()
        };
        let out = preview(b"n\n1\n2\n3\n", cfg);
        assert_eq!(out, "n\n1\n");
    }

    #[test]
    fn no_header_numbers_the_columns() {
        let cfg = Config {
            has_header: false,
            ..Config::default()
        };
        let out = preview(b"ann,3\nbob,4\n", cfg);
        assert_eq!(out, "1    2\nann  3\nbob  4\n");
    }

    #[test]
    fn blank_header_gets_an_index() {
        let out = preview(b"name,,city\nann,x,paris\n", Config::default());
        assert_eq!(out, "name  2  city\nann   x  paris\n");
    }

    #[test]
    fn selects_columns_by_name_or_index() {
        let data = b"name,age,city\nann,3,paris\nbob,4,rome\n";
        let by_name = Config {
            columns: Some(vec!["city".into(), "name".into()]),
            ..Config::default()
        };
        assert_eq!(
            preview(data, by_name),
            "city   name\nparis  ann\nrome   bob\n"
        );
        let by_index = Config {
            columns: Some(vec!["2".into()]),
            ..Config::default()
        };
        assert_eq!(preview(data, by_index), "age\n3\n4\n");
    }

    #[test]
    fn a_name_wins_over_the_same_index() {
        let data = b"age,1\n3,x\n";
        let cfg = Config {
            columns: Some(vec!["1".into()]),
            ..Config::default()
        };
        assert_eq!(preview(data, cfg), "1\nx\n");
    }

    #[test]
    fn missing_column_is_an_error() {
        let mut out = Vec::new();
        let cfg = Config {
            columns: Some(vec!["nope".into()]),
            ..Config::default()
        };
        let err = run(b"name,age\nann,3\n" as &[u8], &mut out, &cfg).unwrap_err();
        assert_eq!(err.to_string(), "column not found: nope");
    }

    #[test]
    fn empty_input_prints_nothing() {
        assert_eq!(preview(b"", Config::default()), "");
    }

    struct CountingRead {
        data: Vec<u8>,
        pos: usize,
        bytes: Rc<Cell<usize>>,
    }

    impl Read for CountingRead {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let n = buf.len().min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            self.bytes.set(self.bytes.get() + n);
            Ok(n)
        }
    }

    #[test]
    fn preview_does_not_read_the_rest_of_a_large_file() {
        let mut data = String::from("n,v\n");
        for i in 0..50_000 {
            data.push_str(&format!("{i},xxxxxxxxxxxxxxxx\n"));
        }
        let data = data.into_bytes();
        assert!(data.len() > SAMPLE);
        let bytes = Rc::new(Cell::new(0));
        let src = CountingRead {
            data,
            pos: 0,
            bytes: Rc::clone(&bytes),
        };
        let cfg = Config {
            rows: 2,
            ..Config::default()
        };
        let mut out = Vec::new();
        run(src, &mut out, &cfg).unwrap();
        assert!(bytes.get() <= SAMPLE, "read {} bytes", bytes.get());
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 3);
    }
}
