use anchor_lang::prelude::*;

pub fn pk(b: u8) -> Pubkey {
    Pubkey::new_from_array([b; 32])
}

pub fn anchor_error_code<E>(err: E) -> u32
where
    E: Into<u32>,
{
    err.into()
}

#[cfg(test)]
pub(crate) fn make_account_info<'a>(
    key: &'a Pubkey,
    owner: &'a Pubkey,
    lamports: &'a mut u64,
    data: &'a mut [u8],
) -> AccountInfo<'a> {
    AccountInfo::new(key, false, false, lamports, data, owner, false)
}

#[cfg(test)]
pub fn assert_anchor_error<T, E>(result: anchor_lang::Result<T>, expected: E)
where
    E: Into<u32> + Copy + std::fmt::Debug,
{
    let expected_code = anchor_error_code(expected);

    match result {
        Ok(_) => {
            panic!(
                "Expected error {:?} (code {}), but call succeeded",
                expected, expected_code,
            );
        }

        Err(err) => match err {
            anchor_lang::error::Error::AnchorError(anchor_err) => {
                assert_eq!(
                    anchor_err.error_code_number, expected_code,
                    "Anchor error mismatch.\nExpected: {:?} ({})\nActual: {} ({})",
                    expected, expected_code, anchor_err.error_name, anchor_err.error_code_number,
                );
            }

            other => {
                panic!(
                    "Expected AnchorError {:?} ({}), got different error:\n{:?}",
                    expected, expected_code, other,
                );
            }
        },
    }
}
