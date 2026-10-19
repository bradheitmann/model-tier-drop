//! The CLI's clock: RFC 3339 UTC timestamps and ULID-shaped ids. The engine has no clock;
//! every timestamp and generated id it sees comes from here.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Crockford base32 alphabet used by ULIDs.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn since_epoch() -> std::time::Duration {
    // decision: a clock set before 1970 is treated as the epoch rather than aborting.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// Civil date (year, month, day) from days since 1970-01-01 (Howard Hinnant's algorithm).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a count of seconds since the Unix epoch.
pub fn rfc3339_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// The current time as `YYYY-MM-DDTHH:MM:SSZ`.
// decision: whole seconds only, with a `Z` suffix, because the engine and the CLI compare
// timestamps as strings; mixing `…:00Z` and `…:00.5Z` would sort `.` before `Z` and break order.
pub fn now_rfc3339() -> String {
    rfc3339_from_secs(since_epoch().as_secs())
}

/// Whether `s` has the exact `YYYY-MM-DDTHH:MM:SSZ` shape the CLI compares as strings.
pub fn is_rfc3339_utc_seconds(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// A 26-character ULID-shaped id: 48 bits of milliseconds, then 80 bits from a xorshift
/// generator seeded from the clock, the process id and a per-process counter.
pub fn ulid() -> String {
    let now = since_epoch();
    let millis = (now.as_millis() as u64) & 0xFFFF_FFFF_FFFF;
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut seed = (now.as_nanos() as u64)
        ^ (u64::from(std::process::id()) << 32)
        ^ count.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    if seed == 0 {
        seed = 0x2545_F491_4F6C_DD1D;
    }
    let a = xorshift64(seed);
    let b = xorshift64(a);
    let random: u128 = ((u128::from(a) << 64) | u128::from(b)) & ((1u128 << 80) - 1);
    let value: u128 = (u128::from(millis) << 80) | random;
    let mut out = [0u8; 26];
    for (i, slot) in out.iter_mut().enumerate() {
        let shift = 5 * (25 - i);
        *slot = CROCKFORD[((value >> shift) & 0x1F) as usize];
    }
    out.iter().map(|c| *c as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_known_instants() {
        assert_eq!(rfc3339_from_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_secs(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_from_secs(1_792_400_461), "2026-10-19T09:01:01Z");
        assert!(is_rfc3339_utc_seconds(&now_rfc3339()));
        assert!(!is_rfc3339_utc_seconds("2026-10-19T09:01:01.5Z"));
        assert!(!is_rfc3339_utc_seconds("2026-10-19T09:01:01+00:00"));
    }

    #[test]
    fn ulid_shape_and_uniqueness() {
        let a = ulid();
        let b = ulid();
        assert_eq!(a.len(), 26);
        assert!(a.bytes().all(|c| CROCKFORD.contains(&c)));
        assert_ne!(a, b);
    }
}
