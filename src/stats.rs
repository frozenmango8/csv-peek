use std::collections::BTreeSet;

use crate::table;

/// Exact unique counts keep every value. Past this many uniques, estimate
/// cardinality from the k smallest hashes instead (k-minimum values).
const DISTINCT_CAP: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Empty,
    Int,
    Float,
    Bool,
    String,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Empty => "empty",
            Kind::Int => "integer",
            Kind::Float => "float",
            Kind::Bool => "boolean",
            Kind::String => "string",
        }
    }
}

pub(crate) struct Summary {
    cols: Vec<Col>,
    rows: u64,
}

impl Summary {
    pub(crate) fn new(names: &[String]) -> Self {
        Self {
            cols: names.iter().map(|n| Col::new(n.clone())).collect(),
            rows: 0,
        }
    }

    pub(crate) fn observe(&mut self, fields: &csv::StringRecord, idxs: &[usize]) {
        self.rows += 1;
        for (col, &i) in self.cols.iter_mut().zip(idxs.iter()) {
            col.observe(fields.get(i).unwrap_or(""));
        }
    }

    pub(crate) fn render(&self, width: usize) -> String {
        let header = Vec::from(
            ["column", "type", "empty", "distinct", "min", "max", "mean"].map(str::to_string),
        );
        let rows = self.cols.iter().map(Col::cells).collect::<Vec<_>>();
        let mut out = format!("rows: {}\n", self.rows);
        out.push_str(&table::render(&header, &rows, width));
        out
    }
}

struct Col {
    name: String,
    empty: u64,
    kind: Kind,
    distinct: Distinct,
    nums: Nums,
}

impl Col {
    fn new(name: String) -> Self {
        Self {
            name,
            empty: 0,
            kind: Kind::Empty,
            distinct: Distinct::default(),
            nums: Nums::default(),
        }
    }

    fn observe(&mut self, raw: &str) {
        let value = raw.trim();
        if value.is_empty() {
            self.empty += 1;
            return;
        }
        self.distinct.observe(value);
        let next = classify(value);
        self.kind = merge(self.kind, next);
        match self.kind {
            Kind::Int => match value.parse::<i128>() {
                Ok(n) => self.nums.push_int(n),
                Err(_) => self.nums.invalidate(),
            },
            Kind::Float => {
                self.nums.i_ok = false;
                match parse_finite(value) {
                    Some(n) => self.nums.push_float(n),
                    None => self.nums.f_ok = false,
                }
            }
            _ => self.nums.invalidate(),
        }
    }

    fn cells(&self) -> Vec<String> {
        let (min, max, mean) = self.range();
        vec![
            self.name.clone(),
            self.kind.label().to_string(),
            self.empty.to_string(),
            self.distinct.display(),
            min,
            max,
            mean,
        ]
    }

    fn range(&self) -> (String, String, String) {
        match self.kind {
            Kind::Int if self.nums.i_ok && self.nums.n_i > 0 => (
                self.nums.imin.to_string(),
                self.nums.imax.to_string(),
                fmt_int_mean(self.nums.isum, self.nums.n_i),
            ),
            Kind::Float if self.nums.f_ok && self.nums.n_f > 0 => (
                fmt_num(self.nums.fmin),
                fmt_num(self.nums.fmax),
                fmt_num(self.nums.fsum / self.nums.n_f as f64),
            ),
            _ => (String::new(), String::new(), String::new()),
        }
    }
}

fn classify(s: &str) -> Kind {
    if is_int(s) {
        Kind::Int
    } else if parse_finite(s).is_some() {
        Kind::Float
    } else if is_bool(s) {
        Kind::Bool
    } else {
        Kind::String
    }
}

fn is_int(s: &str) -> bool {
    let digits = match s.as_bytes().first() {
        Some(b'+' | b'-') => &s[1..],
        _ => s,
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn is_bool(s: &str) -> bool {
    ["true", "false", "yes", "no", "t", "f"]
        .iter()
        .any(|word| s.eq_ignore_ascii_case(word))
}

fn parse_finite(s: &str) -> Option<f64> {
    match s.parse::<f64>() {
        Ok(n) if n.is_finite() => Some(n),
        _ => None,
    }
}

fn merge(cur: Kind, next: Kind) -> Kind {
    match cur {
        Kind::Empty => next,
        Kind::String => Kind::String,
        Kind::Int => match next {
            Kind::Int => Kind::Int,
            Kind::Float => Kind::Float,
            _ => Kind::String,
        },
        Kind::Float => match next {
            Kind::Int | Kind::Float => Kind::Float,
            _ => Kind::String,
        },
        Kind::Bool => match next {
            Kind::Bool => Kind::Bool,
            _ => Kind::String,
        },
    }
}

struct Nums {
    imin: i128,
    imax: i128,
    isum: i128,
    n_i: u64,
    i_ok: bool,
    fmin: f64,
    fmax: f64,
    fsum: f64,
    n_f: u64,
    f_ok: bool,
}

impl Default for Nums {
    fn default() -> Self {
        Self {
            imin: 0,
            imax: 0,
            isum: 0,
            n_i: 0,
            i_ok: true,
            fmin: 0.0,
            fmax: 0.0,
            fsum: 0.0,
            n_f: 0,
            f_ok: true,
        }
    }
}

impl Nums {
    fn invalidate(&mut self) {
        self.i_ok = false;
        self.f_ok = false;
    }

    fn push_int(&mut self, n: i128) {
        if self.n_i == 0 {
            self.imin = n;
            self.imax = n;
        } else {
            self.imin = self.imin.min(n);
            self.imax = self.imax.max(n);
        }
        self.n_i += 1;
        match self.isum.checked_add(n) {
            Some(sum) => self.isum = sum,
            None => self.i_ok = false,
        }
        match exact_f64(n) {
            Some(f) => self.push_float(f),
            None => self.f_ok = false,
        }
    }

    fn push_float(&mut self, n: f64) {
        if self.n_f == 0 {
            self.fmin = n;
            self.fmax = n;
        } else {
            self.fmin = self.fmin.min(n);
            self.fmax = self.fmax.max(n);
        }
        self.n_f += 1;
        self.fsum += n;
        if !self.fsum.is_finite() {
            self.f_ok = false;
        }
    }
}

fn exact_f64(n: i128) -> Option<f64> {
    let f = n as f64;
    if f.is_finite() && f as i128 == n {
        Some(f)
    } else {
        None
    }
}

fn fmt_int_mean(sum: i128, n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    let n = i128::from(n);
    if sum % n == 0 {
        (sum / n).to_string()
    } else {
        fmt_num(sum as f64 / n as f64)
    }
}

fn fmt_num(n: f64) -> String {
    let plain = format!("{n}");
    if plain.len() <= 12 {
        return plain;
    }
    let fixed = trim_zeros(&format!("{n:.4}"));
    if fixed.len() <= 12 && fixed != "0" && fixed != "-0" {
        return fixed;
    }
    trim_zeros(&format!("{n:.4e}"))
}

fn trim_zeros(s: &str) -> String {
    if let Some((mant, exp)) = s.split_once('e') {
        let mant = mant.trim_end_matches('0').trim_end_matches('.');
        format!("{mant}e{exp}")
    } else {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[derive(Default)]
struct Distinct {
    hashes: BTreeSet<u64>,
    saturated: bool,
}

impl Distinct {
    fn observe(&mut self, value: &str) {
        let hash = hash_str(value);
        if self.hashes.contains(&hash) {
            return;
        }
        if self.hashes.len() < DISTINCT_CAP {
            self.hashes.insert(hash);
            return;
        }
        self.saturated = true;
        let Some(last) = self.hashes.iter().next_back().copied() else {
            return;
        };
        if hash < last {
            self.hashes.remove(&last);
            self.hashes.insert(hash);
        }
    }

    fn estimate(&self) -> (u64, bool) {
        if !self.saturated {
            return (self.hashes.len() as u64, true);
        }
        let Some(kth) = self.hashes.iter().next_back().copied() else {
            return (0, false);
        };
        if kth == 0 {
            return (DISTINCT_CAP as u64, false);
        }
        // (k - 1) / (kth / 2^64). u64::MAX rounds to 2^64 in f64.
        let est = (DISTINCT_CAP as f64 - 1.0) * (u64::MAX as f64) / (kth as f64);
        let est = est.round().max(DISTINCT_CAP as f64);
        if est >= u64::MAX as f64 {
            (u64::MAX, false)
        } else {
            (est as u64, false)
        }
    }

    fn display(&self) -> String {
        let (n, exact) = self.estimate();
        if exact {
            n.to_string()
        } else {
            format!("~{n}")
        }
    }
}

fn hash_str(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    // SplitMix finalizer. KMV wants hashes spread across the whole u64 range.
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
    h ^ (h >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(values: &[&str]) -> Col {
        let mut col = Col::new("c".into());
        for v in values {
            col.observe(v);
        }
        col
    }

    fn cells(values: &[&str]) -> Vec<String> {
        column(values).cells()
    }

    #[test]
    fn types_and_numeric_range() {
        assert_eq!(
            cells(&["10", "30", " 20 "]),
            vec!["c", "integer", "0", "3", "10", "30", "20"]
        );
        assert_eq!(
            cells(&["1.5", "2.5", "3.5"]),
            vec!["c", "float", "0", "3", "1.5", "3.5", "2.5"]
        );
        assert_eq!(
            cells(&["10", "1.5"]),
            vec!["c", "float", "0", "2", "1.5", "10", "5.75"]
        );
        assert_eq!(
            cells(&["true", "FALSE", "yes"]),
            vec!["c", "boolean", "0", "3", "", "", ""]
        );
        assert_eq!(
            cells(&["ann", "bob"]),
            vec!["c", "string", "0", "2", "", "", ""]
        );
        assert_eq!(
            cells(&["", "  ", ""]),
            vec!["c", "empty", "3", "0", "", "", ""]
        );
    }

    #[test]
    fn mixed_values_become_strings_without_a_range() {
        let got = cells(&["10", "nope"]);
        assert_eq!(got[1], "string");
        assert_eq!(&got[4..], &["", "", ""]);
    }

    #[test]
    fn integer_mean_keeps_a_fraction() {
        assert_eq!(cells(&["1", "2"])[6], "1.5");
    }

    #[test]
    fn long_mean_is_rounded() {
        assert_eq!(cells(&["1", "2", "2"])[6], "1.6667");
    }

    #[test]
    fn empty_cells_are_not_distinct() {
        let got = cells(&["a", "", "a", " "]);
        assert_eq!(got[2], "2");
        assert_eq!(got[3], "1");
    }

    #[test]
    fn huge_integer_has_no_range() {
        let big = "9".repeat(50);
        let got = cells(&[&big]);
        assert_eq!(got[1], "integer");
        assert_eq!(&got[4..], &["", "", ""]);
    }

    #[test]
    fn distinct_is_exact_until_the_cap() {
        let mut d = Distinct::default();
        for i in 0..100 {
            let v = format!("v{i}");
            d.observe(&v);
            d.observe(&v);
        }
        assert_eq!(d.estimate(), (100, true));
    }

    #[test]
    fn distinct_estimate_is_in_the_right_ballpark() {
        let mut d = Distinct::default();
        for i in 0..5_000 {
            d.observe(&format!("v{i}"));
        }
        let (n, exact) = d.estimate();
        assert!(!exact);
        let err = (n as f64 - 5_000.0).abs() / 5_000.0;
        assert!(err < 0.15, "estimate {n}");
    }
}
