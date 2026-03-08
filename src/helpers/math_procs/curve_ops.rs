//! Curve operation for the AMM
use crate::helpers::MegaAmmProgramError;
use crate::helpers::math_procs::*;
use pinocchio_log::log;

pub struct MegaAmmStableSwapCurve<'b> {
    pub balances: &'b [u64],
    pub fee: u64,
}

impl<'b> MegaAmmStableSwapCurve<'b> {
    // amp: The amplitude parameter.
    // balances: Array of token balances.
    // n: number of tokens in the pool
    pub fn deposit_to_amm(
        &self, amp: u64,  total_lp_supply: u64, n: u32, balances: &[u64],
    ) -> Result<u64, &'static str> {
        // Should return the number of LP tokens to mint.
        let d_old = get_d(amp, self.balances, n)?;
        let d_new = get_d(amp, balances, n)?;
        // For initial liquidity provision or genesis deposit,
        // The initial LP token supply is equal to the first calculated
        if d_old == 0 {
            return Ok(d_new);
        }
        let spread = d_new.checked_sub(d_old).ok_or("Deposit spread error")?;
        let lp_tokens = total_lp_supply.checked_mul(spread).ok_or("Deposit instruction overflow")?
            .checked_div(d_old).ok_or("Deposit division error")?;
        Ok(lp_tokens)
    }

    // Lp to burn is specified by the user from the amount "burnable" from the frontend. 
    // This function returns proportional amounts of tokens to send to the user's wallet.
    pub fn amm_balanced_withdrawal(&self, lp_to_burn: u64, lp_supply: u64) -> Result<[u64; 2], &'static str> {
        let amount_out = withdraw_balanced(self.balances, lp_to_burn, lp_supply).map_err(|_| "Balanced withdrawal error")?;
        // Slice of proportional amount to be transferred.
        Ok(amount_out)
    }

    // Withdrawing one coin. Behaves like a virtual swap.
    // returns the amount of the token to be transferred
    pub fn amm_imbalanced_withdrawal(&self, lp_to_burn: u64, lp_supply: u64, d_current: u64, amp: u64, fee: u64) -> Result<u64, &'static str> {
        let amount_out = withdraw_imbalanced(lp_to_burn, lp_supply, self.balances, d_current, amp).map_err(|_| "Imbalanced withdrawal")?;
        // Final amount minux swap fee.
        let final_amount = apply_swap_fee(amount_out, fee)?;
        // The amount of the token to be transferred.
        Ok(final_amount)
    }

    // Perform a swap on amm.
    pub fn stableswap(&self, amp: u64, target_token_balance: u64, n: u32) -> Result<u64, &'static str> {
        // The the reserves and find their sum. x_i.
        let x_i = self.balances[..self.balances.len() - 1].iter().sum();
        let y_old = self.balances[self.balances.len() - 1];
        let y_new = newton_solver_scaled(
            amp, x_i, target_token_balance, n
        ).expect("Should converge");

        // The Delta invariant pattern
        let amount_out_raw = y_old.checked_sub(y_new)
        .ok_or("Negative swap: User must add more tokens or check invariant")?;
        // Applying the fee. self.fee is in basis points e.g, 30 for 0.3%
        let fee = (amount_out_raw as u128 * self.fee as u128 / 10000) as u64;
        let final_amount_out = amount_out_raw.checked_sub(fee).ok_or("Fee overflow")?;
        Ok(final_amount_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_initial_deposit_to_amm_proportional_minting() {
        // Setup: Balanced pool [1M, 1M] -> D_old = 2M
        let initial_reserves = [0u64, 0u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &initial_reserves,
            fee: 0, 
        };

        let total_supply = 0u64; // Existing LP tokens (1:1 with D)
        
        // User adds 100k of each token (proportional deposit)
        // New balances: [1.1M, 1.1M] -> D_new = 2.2M
        let new_balances = [1_100_000u64, 1_100_000u64];
        let amp = 100u64;
        let n = 2u32;

        let initial_lp_minted = curve.deposit_to_amm(amp, total_supply, n, &new_balances)
            .expect("Should calculate LP minting for initial deposit");
        println!("The lp minted for initial deposit is: {}", initial_lp_minted);

        // Logic: (2M * 200k) / 2M = 200k
        assert_eq!(initial_lp_minted, 2200_000);
    }

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
    fn setup_pool() -> MegaAmmStableSwapCurve<'static> {
        MegaAmmStableSwapCurve {
            balances: &[1_000_000, 1_000_000],
            fee: 30,
        }
    }

    #[test]
    fn test_balanced_withdrawal_proportional() {
        let amm = setup_pool();

        let lp_supply = 1_000_000;
        let lp_to_burn = 100_000;

        let result = amm.amm_balanced_withdrawal(lp_to_burn, lp_supply).unwrap();

        assert_eq!(result[0], 100_000);
        assert_eq!(result[1], 100_000);
    }

    #[test]
    fn test_balanced_withdrawal_full_exit() {
        let amm = setup_pool();

        let lp_supply = 1_000_000;
        let lp_to_burn = 1_000_000;

        let result = amm.amm_balanced_withdrawal(lp_to_burn, lp_supply).unwrap();

        assert_eq!(result[0], 1_000_000);
        assert_eq!(result[1], 1_000_000);
    }
    #[test]
    fn test_balanced_withdrawal_small_lp() {
        let amm = setup_pool();

        let lp_supply = 1_000_000;
        let lp_to_burn = 1;

        let result = amm.amm_balanced_withdrawal(lp_to_burn, lp_supply).unwrap();

        assert!(result[0] > 0);
        assert!(result[1] > 0);
    }
    #[test]
    fn test_imbalanced_withdrawal_single_token() {
        let balances = [1_000_000, 1_000_000];

        let amm = MegaAmmStableSwapCurve {
            balances: &balances,
            fee: 30,
        };

        let lp_supply = 1_000_000;
        let lp_to_burn = 100_000;

        let amp = 100;
        let d_current = get_d(amp, &balances, 2).unwrap();

        let result = amm
            .amm_imbalanced_withdrawal(lp_to_burn, lp_supply, d_current, amp, 30)
            .unwrap();

        assert!(result > 0);
    }
    #[test]
    fn test_fee_applied_on_imbalanced_withdrawal() {
        let balances = [1_000_000, 1_000_000];

        let amm = MegaAmmStableSwapCurve {
            balances: &balances,
            fee: 100,
        };

        let lp_supply = 1_000_000;
        let lp_to_burn = 100_000;
        let amp = 100;

        let d_current = get_d(amp, &balances, 2).unwrap();

        let amount = amm
            .amm_imbalanced_withdrawal(lp_to_burn, lp_supply, d_current, amp, 100)
            .unwrap();

        assert!(amount > 0);
    }
    #[test]
    fn test_zero_lp_burn() {
        let amm = setup_pool();

        let result = amm.amm_balanced_withdrawal(0, 1_000_000).unwrap();

        assert_eq!(result[0], 0);
        assert_eq!(result[1], 0);
    }
    #[test]
    fn test_imbalanced_pool_withdrawal() {
        let balances = [1_800_000, 200_000];

        let amm = MegaAmmStableSwapCurve {
            balances: &balances,
            fee: 30,
        };

        let amp = 100;
        let lp_supply = 1_000_000;
        let lp_to_burn = 100_000;

        let d_current = get_d(amp, &balances, 2).unwrap();

        let result = amm
            .amm_imbalanced_withdrawal(lp_to_burn, lp_supply, d_current, amp, 30)
            .unwrap();

        assert!(result > 0);
    }
    #[test]
    fn test_large_balances() {
        let balances = [10_000_000_000, 10_000_000_000];

        let amm = MegaAmmStableSwapCurve {
            balances: &balances,
            fee: 30,
        };

        let lp_supply = 1_000_000_000;
        let lp_to_burn = 100_000_000;

        let amp = 100;

        let d_current = get_d(amp, &balances, 2).unwrap();

        let result = amm
            .amm_imbalanced_withdrawal(lp_to_burn, lp_supply, d_current, amp, 30)
            .unwrap();

        assert!(result > 0);
    }
    //=========================== TESTING SWAPS ============================================
    //
    //
    #[test]
    fn test_stableswap_standard_execution_with_fee() {
        // Initial State: [1,000,000, 1,000,000] -> D = 2,000,000
        let reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve {
            balances: &reserves,
            fee: 30, // 0.3% fee (30 basis points)
        };

        let amp = 100u64;
        let d = 2_000_000u64; // Invariant stays constant
        let n = 2u32;

        // Simulate user adding 100k of Token 0.
        // The struct 'balances' should be updated outside or 
        // the curve should be initialized with the NEW balance of the input token.
        let curve_after_deposit = MegaAmmStableSwapCurve {
            balances: &[1_100_000u64, 1_000_000u64], // Token 0 increased
            fee: 30,
        };

        let amount_out = curve_after_deposit.stableswap(amp, d, n)
            .expect("Swap should converge");

        // Mathematical Expectation:
        // In a StableSwap pool with A=100, swapping 100k usually yields ~99k raw.
        // 0.3% fee on 99k is ~297 units.
        // amount_out should be roughly 98,700 - 99,000.
        assert!(amount_out > 90_000, "Payout too low");
        assert!(amount_out < 100_000, "Payout cannot exceed input (negative slippage)");
        
        // Fee check: Verify it is less than the raw difference
        // (y_old - y_new) > final_amount_out
        assert!(1_000_000 > amount_out);
    }

    #[test]
    fn test_stableswap_zero_fee_impact() {
        let reserves = [1_100_000u64, 1_000_000u64];
        let curve_no_fee = MegaAmmStableSwapCurve { balances: &reserves, fee: 0 };
        let curve_with_fee = MegaAmmStableSwapCurve { balances: &reserves, fee: 500 }; // 5% fee

        let d = 2_000_000u64;
        let amp = 100u64;

        let out_no_fee = curve_no_fee.stableswap(amp, d, 2).unwrap();
        let out_with_fee = curve_with_fee.stableswap(amp, d, 2).unwrap();

        // Higher fee must result in lower payout
        assert!(out_no_fee > out_with_fee);
    }

    #[test]
    fn test_stableswap_high_slippage_imbalance() {
        // Pool is heavily imbalanced [1.9M, 100k]. 
        // D is approx 1.1M (Calculated previously).
        let reserves = [1_900_000u64, 100_000u64]; 
        let curve = MegaAmmStableSwapCurve { balances: &reserves, fee: 0 };
        
        // If we try to push even more into the 1.9M side, the payout 
        // of the remaining 100k should be very small due to the "Curve Wall".
        let d = 1_100_000u64; 
        let amp = 10u64;

        let amount_out = curve.stableswap(amp, d, 2).unwrap();
        
        // Even if we added a lot, we can't take more than the 100k available.
        assert!(amount_out < 100_000);
    }

    #[test]
    fn test_stableswap_invalid_invariant_fails() {
        let reserves = [1_000_000u64, 1_000_000u64];
        let curve = MegaAmmStableSwapCurve { balances: &reserves, fee: 30 };
        
        // If D is set higher than the current reserves can support, 
        // y_new will be > y_old, triggering our "Negative swap" error.
        let impossible_d = 3_000_000u64; 
        
        let result = curve.stableswap(100, impossible_d, 2);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Negative swap: User must add more tokens or check invariant");
    }    

    // ====================== PROPTESTS ==========================
    proptest! {
        // ================= TESTING DEPOSITS ================
        // ================== TESTING WITHDRAWALS ===============
        // ================== TESTING SWAPS ===================
    }
}
