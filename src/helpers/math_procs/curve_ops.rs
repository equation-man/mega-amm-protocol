//! Curve operation for the AMM
use crate::helpers::MegaAmmProgramError;
use crate::helpers::math_procs::*;

pub struct MegaAmmStableSwapCurve {
    vault_x_balance: u64,
    vault_y_balance: u64,
    fee: u64,
}

impl MegaAmmStableSwapCurve {
    pub fn deposit_to_amm(
        new_token_a: u64, new_token_b: u64,
        total_lp_supply: u64, amp: u64, reserve: &[u64]
    ) -> Result<(u64, u64, u64), &'static str> {
        //let n = reserve.len() as u32;

        // Retrieve previous balances from old_reserve.
        let mut reserve_out: [u128; 2] = [0 as u128; 2];
        u64_to_u128_inplace(reserve, &mut reserve_out);
        let d0 = safeguarded_newton_solver(&reserve_out, amp as u128)?;

        // Well compute d1 with new reserve values with updated tokens.
        let new_t_a = reserve[0].checked_add(new_token_a).ok_or("Error adding token a")?;
        let new_t_b = reserve[1].checked_add(new_token_b).ok_or("Error adding token b")?;
        let new_reserve: [u128; 2] = [u128::from(new_t_a), u128::from(new_t_b)];
        let d1 = safeguarded_newton_solver(&new_reserve, amp as u128)?;

        let mint_amount = if total_lp_supply == 0 {
            d1
        } else {
            if d1 <= d0 {
                return Err("Invariant did not increase");
            }

            let epsilon = d1.checked_sub(d0).ok_or("Subtraction error on curve")?;
            (total_lp_supply)
                .checked_mul(epsilon).ok_or("Overflow error")?
                .checked_div(d0).ok_or("Division error")?

        };

        Ok((new_t_a, new_t_b, mint_amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Dummy implementation of u64_to_u128_inplace for testing
    fn u64_to_u128_inplace(input: &[u64], output: &mut [u128; 2]) {
        output[0] = input[0] as u128;
        output[1] = input[1] as u128;
    }

    // Dummy safeguarded_newton_solver for testing
    fn safeguarded_newton_solver(reserve: &[u128; 2], amp: u128) -> Result<u128, &'static str> {
        // Simple approximation: D = sum(reserve) * amp / 1000 + 1 to avoid zero
        Ok(reserve.iter().sum::<u128>() * amp / 1000 + 1)
    }

    #[test]
    #[ignore]
    fn test_bootstrap_deposit() {
        let reserve: [u64; 2] = [0, 0];
        let new_token_a = 1000u64;
        let new_token_b = 1000u64;
        let total_lp_supply = 0u64;
        let amp = 100u64;

        let result = MegaAmmStableSwapCurve::deposit_to_amm(
            new_token_a,
            new_token_b,
            total_lp_supply,
            amp,
            &reserve,
        )
        .unwrap();

        println!("Bootstrap deposit result: {:?}", result);
        let (new_a, new_b, mint) = result;
        assert_eq!(new_a, 1000);
        assert_eq!(new_b, 1000);
        assert!(mint > 0);
    }

    #[test]
    fn test_regular_deposit() {
        let reserve: [u64; 2] = [5000, 5000];
        let new_token_a = 1000u64;
        let new_token_b = 1000u64;
        let total_lp_supply = 10000u64;
        let amp = 100u64;

        let result = MegaAmmStableSwapCurve::deposit_to_amm(
            new_token_a,
            new_token_b,
            total_lp_supply,
            amp,
            &reserve,
        )
        .unwrap();

        println!("Regular deposit result: {:?}", result);
        let (new_a, new_b, mint) = result;
        assert_eq!(new_a, 6000);
        assert_eq!(new_b, 6000);
        assert!(mint > 0 && mint < total_lp_supply);
    }

    #[test]
    fn test_small_deposit_does_not_increase_invariant() {
        let reserve: [u64; 2] = [1000, 1000];
        let new_token_a = 0u64;
        let new_token_b = 0u64;
        let total_lp_supply = 1000u64;
        let amp = 100u64;

        let result = MegaAmmStableSwapCurve::deposit_to_amm(
            new_token_a,
            new_token_b,
            total_lp_supply,
            amp,
            &reserve,
        );
        println!("Testing small deposit {:?}", result);

        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Invariant did not increase");
    }
}
