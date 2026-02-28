//! Curve operation for the AMM
use crate::helpers::MegaAmmProgramError;
use crate::helpers::math_procs::*;

pub struct MegaAmmStableSwapCurve<'b> {
    balances: &'b [u64],
    fee: u64,
}

impl<'b> MegaAmmStableSwapCurve<'b> {
    // amp: The amplitude parameter.
    // balances: Array of token balances.
    // n: number of tokens in the pool
    pub fn deposit_to_amm(
        &self, amp: u64,  total_lp_supply: u64, n: u32, balances: &[u64],
    ) -> Result<u64, &'static str> {
        // Should return the number of LP tokens to mint.
        let d_old = deposit_liquidity(amp, self.balances, n)?;
        let d_new = deposit_liquidity(amp, balances, n)?;
        let spread = d_new.checked_sub(d_old).ok_or("Deposit spread error")?;
        let lp_tokens = total_lp_supply.checked_mul(spread).ok_or("Deposit instruction overflow")?
            .checked_div(d_old).ok_or("Deposit division error")?;
        Ok(lp_tokens)
    }

    // Withdrawing from amm, from balanced or unbalanced pools
    pub fn withdraw_from_amm(
        &self, lp_to_burn: u64, total_lp_supply: u64,
        balanced_state: u8, d_current: Option<u64>, amp: Option<u64>
    ) -> Result<[u64; 2], &'static str> {
        // Returns the final payout amount to be transferred.
        match balanced_state {
            0 => {
                let amount_out = withdraw_balanced(
                    self.balances, lp_to_burn, total_lp_supply
                ).expect("Balanced withdrawal error");
                Ok(amount_out)
            },
            1 => {
                let amount_out = withdraw_imbalanced(
                    lp_to_burn, total_lp_supply, self.balances,
                    d_current.unwrap(), amp.unwrap()
                ).expect("Balanced withdrawal error");
                Ok([amount_out, 0 as u64])
            },
            _ => Err("Withdraw mode error")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_to_amm_proportional_minting() {
        // Setup: Balanced pool [1M, 1M] -> D_old = 2M
        let initial_reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &initial_reserves,
            fee: 0, 
        };

        let total_supply = 2_000_000u64; // Existing LP tokens (1:1 with D)
        
        // User adds 100k of each token (proportional deposit)
        // New balances: [1.1M, 1.1M] -> D_new = 2.2M
        let new_balances = [1_100_000u64, 1_100_000u64];
        let amp = 100u64;
        let n = 2u32;

        let lp_minted = curve.deposit_to_amm(amp, total_supply, n, &new_balances)
            .expect("Should calculate LP minting for proportional deposit");
        println!("The lp minted for proprotional minting is: {}", lp_minted);

        // Logic: (2M * 200k) / 2M = 200k
        assert_eq!(lp_minted, 200_000);
    }

    #[test]
    fn test_imbalanced_deposit_penalty() {
        // Adding liquidity imbalanced "hurts" the pool's virtual depth.
        // The user should get FEWER LP tokens than the raw sum of their tokens.
        let initial_reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &initial_reserves,
            fee: 0,
        };

        let total_supply = 2_000_000u64;
        // User adds 200k of Token A but 0 of Token B.
        // Sum of tokens added is 200k, but D_new - D_old will be < 200k.
        let new_balances = [1_200_000u64, 1_000_000u64]; 
        let amp = 10u64; // Lower amp to make penalty visible
        let n = 2u32;

        let lp_minted = curve.deposit_to_amm(amp, total_supply, n, &new_balances).unwrap();
        println!("The lp minted for imbalanced deposit penalty: {}", lp_minted);

        // Due to slippage/imbalance penalty, minted LP < 200k
        assert!(lp_minted < 200_000, "Imbalanced deposit must result in LP penalty");
        assert!(lp_minted > 150_000, "Penalty should not be catastrophic");
    }

    #[test]
    fn test_deposit_with_negative_spread_fails() {
        let initial_reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &initial_reserves,
            fee: 0,
        };

        // Attempting to "deposit" while providing balances smaller than current
        let smaller_balances = [900_000u64, 900_000u64];
        
        let result = curve.deposit_to_amm(100, 2_000_000, 2, &smaller_balances);
        
        assert_eq!(result, Err("Deposit spread error"));
    }

    #[test]
    fn test_high_amp_efficiency_for_deposits() {
        let initial_reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve { balances: &initial_reserves, fee: 0 };
        let new_balances = [1_200_000u64, 1_000_000u64];
        let total_supply = 2_000_000u64;

        // High A makes imbalanced deposits more "efficient" (more LP tokens)
        let lp_low_amp = curve.deposit_to_amm(1, total_supply, 2, &new_balances).unwrap();
        let lp_high_amp = curve.deposit_to_amm(1000, total_supply, 2, &new_balances).unwrap();

        assert!(lp_high_amp > lp_low_amp, "Higher Amp should yield more LP for imbalanced deposits");
    }

    // ==================== TESTING WITHDRAWALS =================
    //
    //
    //
    #[test]
    fn test_withdraw_from_amm_balanced_dispatch() {
        // Setup: [1M, 1M] reserves
        let reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &reserves,
            fee: 0,
        };

        // Mode 0: Balanced. Burning 10% (200k of 2M total supply)
        // Should return [100k, 100k]
        let result = curve.withdraw_from_amm(
            200_000, 
            2_000_000, 
            0,      // balanced_state = 0
            None,   // D not needed
            None    // Amp not needed
        ).expect("Balanced dispatch failed");

        assert_eq!(result[0], 100_000);
        assert_eq!(result[1], 100_000);
    }

    #[test]
    fn test_withdraw_from_amm_imbalanced_dispatch() {
        // Setup: [1M, 1M] reserves
        let reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &reserves,
            fee: 0,
        };

        // Mode 1: Imbalanced. Burning 10% (200k of 2M)
        // We know from previous tests this results in more than 100k of Token 0
        // because of the "Imbalance Premium" logic.
        let result = curve.withdraw_from_amm(
            200_000, 
            2_000_000, 
            1,             // balanced_state = 1
            Some(2_000_000), // d_current
            Some(100)        // amp
        ).expect("Imbalanced dispatch failed");

        // Token 0 should have the payout, Token 1 should be 0 (as per your implementation)
        assert!(result[0] > 100_000);
        assert_eq!(result[1], 0);
    }

    #[test]
    fn test_withdraw_from_amm_invalid_mode() {
        let reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve { balances: &reserves, fee: 0 };

        // Mode 2 is undefined
        let result = curve.withdraw_from_amm(100, 1000, 2, None, None);
        assert_eq!(result, Err("Withdraw mode error"));
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn test_withdraw_imbalanced_missing_params_panics() {
        let reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve { balances: &reserves, fee: 0 };

        // Mode 1 requires Some(d) and Some(amp). Passing None will trigger the unwrap() panic.
        let _ = curve.withdraw_from_amm(200_000, 2_000_000, 1, None, None);
    }
}
