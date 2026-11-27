// Priority order. On a tie, the earlier byte wins.
const CANDIDATES: &[u8] = b",\t;|";

// Excel writes a UTF-8 BOM. Left in place it becomes part of the first header.
pub(crate) fn strip_bom(buf: &mut Vec<u8>) {
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        buf.drain(..3);
    }
}

/// Slice of `buf` the sniffer should see.
///
/// A full sample can end inside a quoted field. When we filled the buffer,
/// drop the trailing partial line so that half a record does not skew the count.
pub(crate) fn prefix(buf: &[u8], sample_cap: usize) -> &[u8] {
    if buf.len() < sample_cap {
        return buf;
    }
    match buf.iter().rposition(|&b| b == b'\n') {
        Some(i) => &buf[..i],
        None => buf,
    }
}

pub(crate) fn sniff_delimiter(sample: &[u8]) -> u8 {
    let mut counts = [0usize; CANDIDATES.len()];
    let mut quoted = false;
    for &b in sample {
        if b == b'"' {
            // RFC 4180 escapes a quote by doubling it, so a toggle stays in sync.
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        if let Some(i) = CANDIDATES.iter().position(|&c| c == b) {
            counts[i] += 1;
        }
    }

    let mut best = 0;
    for i in 1..counts.len() {
        if counts[i] > counts[best] {
            best = i;
        }
    }
    if counts[best] == 0 {
        b','
    } else {
        CANDIDATES[best]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_most_common_delimiter() {
        assert_eq!(sniff_delimiter(b"a,b,c\n1,2,3\n"), b',');
        assert_eq!(sniff_delimiter(b"a\tb\tc\n1\t2\t3\n"), b'\t');
        assert_eq!(sniff_delimiter(b"a;b;c\n1;2;3\n"), b';');
        assert_eq!(sniff_delimiter(b"a|b|c\n1|2|3\n"), b'|');
    }

    #[test]
    fn ignores_delimiters_inside_quotes() {
        let sample = b"name,note\nann,\"a;b|c\tand, more\",x\n";
        assert_eq!(sniff_delimiter(sample), b',');
    }

    #[test]
    fn doubled_quotes_do_not_confuse_the_scan() {
        assert_eq!(sniff_delimiter(b"a,\"b\"\"c\",d\n"), b',');
    }

    #[test]
    fn tie_goes_to_comma_then_tab_then_semicolon() {
        assert_eq!(sniff_delimiter(b"a,b\tc\n"), b',');
        assert_eq!(sniff_delimiter(b"a\tb;c\n"), b'\t');
        assert_eq!(sniff_delimiter(b"a;b|c\n"), b';');
    }

    #[test]
    fn no_delimiter_defaults_to_comma() {
        assert_eq!(sniff_delimiter(b"just-one-column\n"), b',');
        assert_eq!(sniff_delimiter(b""), b',');
    }

    #[test]
    fn prefix_drops_a_partial_trailing_line_only_when_capped() {
        let buf = b"a,b\n1,2\npartial";
        assert_eq!(prefix(buf, buf.len() + 1), buf);
        assert_eq!(prefix(buf, buf.len()), b"a,b\n1,2");
    }

    #[test]
    fn strip_bom_only_removes_a_real_bom() {
        let mut buf = vec![0xEF, 0xBB, 0xBF, b'a'];
        strip_bom(&mut buf);
        assert_eq!(buf, b"a");

        let mut plain = b"abc".to_vec();
        strip_bom(&mut plain);
        assert_eq!(plain, b"abc");
    }
}
