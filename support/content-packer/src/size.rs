/// Parses sizes like `"500M"`, `"1G"`, `"250000000"` into a byte count.
/// Suffixes are decimal (K=1000, M=1000^2, G=1000^3), matching how most
/// release-size limits (e.g. GitHub's) are usually quoted — not `KiB`/`MiB`.
pub fn parse_size(input: &str) -> Result<u64, String> {
    let input = input.trim();
    let (digits, mult) = match input.chars().last() {
        Some(c) if c.eq_ignore_ascii_case(&'k') => (&input[..input.len() - 1], 1_000u64),
        Some(c) if c.eq_ignore_ascii_case(&'m') => (&input[..input.len() - 1], 1_000_000u64),
        Some(c) if c.eq_ignore_ascii_case(&'g') => (&input[..input.len() - 1], 1_000_000_000u64),
        _ => (input, 1u64),
    };
    let value: f64 = digits
        .trim()
        .parse()
        .map_err(|_| format!("invalid size: {input:?}"))?;
    if value < 0.0 {
        return Err(format!("size cannot be negative: {input:?}"));
    }
    Ok((value * mult as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn parses_plain_and_suffixed_sizes() {
        assert_eq!(parse_size("500").unwrap(), 500);
        assert_eq!(parse_size("500M").unwrap(), 500_000_000);
        assert_eq!(parse_size("1.5M").unwrap(), 1_500_000);
        assert_eq!(parse_size("2G").unwrap(), 2_000_000_000);
        assert_eq!(parse_size("10k").unwrap(), 10_000);
    }
}
