//! Human-readable byte counts.

/// Format bytes into a human-readable string (e.g., "3.42 GB").
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format a signed byte delta with an explicit sign (e.g. "+1.50 GB", "-12.00 MB").
///
/// Zero is rendered without a sign ("0 B").
#[must_use]
pub fn format_signed_bytes(bytes: i64) -> String {
    match bytes.cmp(&0) {
        std::cmp::Ordering::Greater => format!("+{}", format_bytes(bytes.unsigned_abs())),
        std::cmp::Ordering::Less => format!("-{}", format_bytes(bytes.unsigned_abs())),
        std::cmp::Ordering::Equal => format_bytes(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_bytes_range() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1_572_864), "1.50 MB"); // 1.5 MB
    }

    #[test]
    fn format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(17_179_869_184), "16.00 GB");
    }

    #[test]
    fn format_signed_bytes_sign_handling() {
        assert_eq!(format_signed_bytes(0), "0 B");
        assert_eq!(format_signed_bytes(1536), "+1.50 KB");
        assert_eq!(format_signed_bytes(-1024 * 1024), "-1.00 MB");
        assert_eq!(format_signed_bytes(i64::MIN).chars().next(), Some('-'));
    }

    #[test]
    fn format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }
}
