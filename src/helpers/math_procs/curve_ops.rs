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

        let mint_amount = if d1 > d0 {
            let epsilon = d1.checked_sub(d0).ok_or("Subtraction error on curve")?;
            (total_lp_supply)
                .checked_mul(epsilon).ok_or("Overflow error")?
                .checked_div(d0).ok_or("Division error")
        } else {
            return Err("Invariant did not increase");
        };

        Ok((new_t_a, new_t_b, mint_amount?))
    }
}
