# csv-peek

[![ci](https://github.com/frozenmango8/csv-peek/actions/workflows/ci.yml/badge.svg)](https://github.com/frozenmango8/csv-peek/actions/workflows/ci.yml)

Preview CSV files and summarize columns from the terminal.

## Install

```
cargo install --git https://github.com/frozenmango8/csv-peek
```

From a checkout: `cargo install --path .`

## Examples

`people.csv`:

```
name,age,city,score
ann,30,paris,1.5
bob,,lyon,2.5
cara,41,"a very long city name",3.5
```

```
$ csv-peek people.csv
name  age  city                   score
ann   30   paris                  1.5
bob        lyon                   2.5
cara  41   a very long city name  3.5
```

```
$ csv-peek -w 32 people.csv
name  age  city            score
ann   30   paris           1.5
bob        lyon            2.5
cara  41   a very long c…  3.5
```

```
$ csv-peek --cols name,score people.csv
name  score
ann   1.5
bob   2.5
cara  3.5
```

```
$ csv-peek --stats people.csv
rows: 3
column  type     empty  distinct  min  max  mean
name    string   0      3
age     integer  1      2         30   41   35.5
city    string   0      3
score   float    0      3         1.5  3.5  2.5
```

Omit the path, or pass `-`, to read stdin. The delimiter is sniffed (comma, tab, semicolon, pipe). `-n` is how many data rows to preview (default 10). `--no-header` treats the first row as data. `-d tab` forces a tab.

A `~` before a distinct count means the figure is an estimate.

## Limitations

- Sniffing only looks at the first 64KiB, outside double quotes. It can guess wrong; pass `-d`.
- Distinct counts are exact until 1024 different values in a column, then a k-minimum-values estimate.
- Min, max, and mean are blank when an integer does not fit in `i128` or the running sum overflows. Floats are `f64`. There is no date type.
- Whitespace-only cells count as empty for stats. The preview prints the raw text.
- Every row has to have the same number of fields. Input is UTF-8; a leading BOM is ignored.
- `--cols` splits on commas, so a header that contains a comma has to be selected by its 1-based index. A header whose name is a number wins over that index.
- The preview stops after N rows. `--stats` reads the whole file.
- If stdout is not a terminal, the width is 80 unless you pass `-w`.
