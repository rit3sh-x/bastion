use anchor_lang::prelude::*;

use crate::error::BastionError;

/// Convert a Unix timestamp (UTC) to `(day_of_week, minute_of_day)`.
///
/// `dow`: 0 = Sunday .. 6 = Saturday (1970-01-01 was a Thursday → +4 offset).
/// `minute_of_day`: 0..=1439.
///
/// Rejects pre-epoch timestamps — TimeOfDayWindow is only meaningful for
/// real wall-clock times.
pub fn dow_minute_of_day(ts: i64) -> Result<(u8, u16)> {
    require!(ts >= 0, BastionError::InvalidPolicyData);
    let ts_u = u64::try_from(ts).map_err(|_| BastionError::NumericalOverflow)?;
    let days = ts_u
        .checked_div(86_400)
        .ok_or(BastionError::NumericalOverflow)?;
    let seconds_today = ts_u
        .checked_rem(86_400)
        .ok_or(BastionError::NumericalOverflow)?;
    let minute_u = seconds_today
        .checked_div(60)
        .ok_or(BastionError::NumericalOverflow)?;
    let dow_u64 = days
        .checked_add(4)
        .ok_or(BastionError::NumericalOverflow)?
        .checked_rem(7)
        .ok_or(BastionError::NumericalOverflow)?;
    let dow = u8::try_from(dow_u64).map_err(|_| BastionError::NumericalOverflow)?;
    let minute = u16::try_from(minute_u).map_err(|_| BastionError::NumericalOverflow)?;
    Ok((dow, minute))
}

/// enforce TimeOfDayWindow policy.
///
/// `start_minute` (inclusive) ≤ now_minute < `end_minute` (exclusive) AND the
/// day-of-week bit must be set in `days_mask`.
pub fn check_time_of_day(ts: i64, start_minute: u16, end_minute: u16, days_mask: u8) -> Result<()> {
    let (dow, now_min) = dow_minute_of_day(ts)?;
    let bit = 1u8
        .checked_shl(u32::from(dow))
        .ok_or(BastionError::NumericalOverflow)?;
    require!((days_mask & bit) != 0, BastionError::OutsideAllowedTime);
    require!(
        now_min >= start_minute && now_min < end_minute,
        BastionError::OutsideAllowedTime
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::utils::general::assert_anchor_error;

    use super::*;
    const MON_2024_01_01_UTC: i64 = 1_704_067_200;
    const SECONDS_PER_DAY: i64 = 86_400;

    #[test]
    fn epoch_is_thursday_zero() {
        let (dow, min) = dow_minute_of_day(0).unwrap();
        assert_eq!(dow, 4);
        assert_eq!(min, 0);
    }

    #[test]
    fn jan_1_2024_is_monday() {
        let (dow, min) = dow_minute_of_day(MON_2024_01_01_UTC).unwrap();
        assert_eq!(dow, 1);
        assert_eq!(min, 0);
    }

    #[test]
    fn minute_advances_correctly() {
        let ts = MON_2024_01_01_UTC + 10 * 60 + 30;
        let (_, min) = dow_minute_of_day(ts).unwrap();
        assert_eq!(min, 10);
    }

    #[test]
    fn passes_in_window_on_allowed_day() {
        let mask = 0x3E;
        let ts = MON_2024_01_01_UTC + 10 * 3600 + 30 * 60; // Mon 10:30 UTC
        assert!(check_time_of_day(ts, 9 * 60, 17 * 60, mask).is_ok());
    }

    #[test]
    fn fails_when_dow_bit_unset() {
        let mask = 0x3E; // Mon-Fri
        let ts = MON_2024_01_01_UTC + 5 * SECONDS_PER_DAY + 10 * 3600; // Sat 10:00
        let res = check_time_of_day(ts, 9 * 60, 17 * 60, mask);
        assert_anchor_error(res, BastionError::OutsideAllowedTime);
    }

    #[test]
    fn fails_before_start_minute() {
        let mask = 0x3E;
        let ts = MON_2024_01_01_UTC + 8 * 3600 + 59 * 60; // Mon 08:59
        let res = check_time_of_day(ts, 9 * 60, 17 * 60, mask);
        assert_anchor_error(res, BastionError::OutsideAllowedTime);
    }

    #[test]
    fn fails_at_end_minute_boundary() {
        let mask = 0x3E;
        let ts = MON_2024_01_01_UTC + 17 * 3600; // Mon 17:00 sharp
        let res = check_time_of_day(ts, 9 * 60, 17 * 60, mask);
        assert_anchor_error(res, BastionError::OutsideAllowedTime);
    }

    #[test]
    fn passes_at_start_minute_boundary() {
        let mask = 0x3E;
        let ts = MON_2024_01_01_UTC + 9 * 3600; // Mon 09:00 sharp
        assert!(check_time_of_day(ts, 9 * 60, 17 * 60, mask).is_ok());
    }

    #[test]
    fn full_week_mask_passes_any_day() {
        let mask = 0x7F;
        let ts = MON_2024_01_01_UTC + 5 * SECONDS_PER_DAY + 12 * 3600;
        assert!(check_time_of_day(ts, 0, 1440, mask).is_ok());
    }

    #[test]
    fn rejects_negative_timestamp() {
        let res = dow_minute_of_day(-1);
        assert_anchor_error(res, BastionError::InvalidPolicyData);
    }
}
