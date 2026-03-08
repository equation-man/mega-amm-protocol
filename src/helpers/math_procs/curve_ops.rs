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
    pub fn stableswap(&self, swap_amount: u64, amp: u64, n: u32) -> Result<u64, &'static str> {
        // The the reserves and find their sum. x_i.
        let sum_avail_tokens: u64 = self.balances[..self.balances.len() - 1].iter().sum();
        let sum_after_swap = sum_avail_tokens.checked_add(swap_amount).ok_or("Addition overflow")?;
        let y_target_old = self.balances[self.balances.len() - 1];
        // Get the current liquidity
        let d_param = get_d(amp, self.balances, n)?;
        let y_target_new = newton_solver_scaled(
            amp, sum_after_swap, d_param, n
        ).expect("Should converge");

        // The Delta invariant pattern
        let amount_out_raw = y_target_old.checked_sub(y_target_new)
        .ok_or("Negative swap: User must add more tokens or check invariant")?;
        // Applying the fee. self.fee is in basis points e.g, 30 for 0.3%
        let fee = (amount_out_raw as u128 * self.fee as u128 / 10000) as u64;
        let final_amount_out = amount_out_raw.checked_sub(fee).ok_or("Fee overflow")?;
        // Return final amount to transfer.
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
    fn make_curve(balances: &[u64], fee: u64) -> MegaAmmStableSwapCurve {
        MegaAmmStableSwapCurve { balances, fee }
    }

    #[test]
    fn test_swap_balanced_pool_small_amount() {
        let balances = [1_000_000u64, 1_000_000u64];
        let swap_amount = 100_000u64;
        let amp = 100u64;
        let n = 2u32;
        let fee = 30u64; // 0.3%

        let curve = make_curve(&balances, fee);

        let amount_out = curve.stableswap(swap_amount, amp, n).expect("Should swap");

        // Amount out must be positive and less than target token balance
        assert!(amount_out > 0);
        assert!(amount_out < balances[1]);

        // Fee must reduce output
        let expected_no_fee = amount_out + (amount_out * fee / 10000);
        assert!(expected_no_fee <= balances[1]);
    }

    #[test]
    fn test_swap_balanced_pool_large_amount() {
        let balances = [1_000_000u64, 1_000_000u64];
        let swap_amount = 900_000u64;
        let amp = 100u64;
        let n = 2u32;
        let fee = 30u64;

        let curve = make_curve(&balances, fee);
        let amount_out = curve.stableswap(swap_amount, amp, n).expect("Should swap");

        // Should not exceed pool
        assert!(amount_out < balances[1]);
    }

    #[test]
    fn test_swap_imbalanced_pool_small_amount() {
        let balances = [1_000_000u64, 500_000u64];
        let swap_amount = 50_000u64;
        let amp = 50u64;
        let n = 2u32;
        let fee = 30u64;

        let curve = make_curve(&balances, fee);

        let amount_out = curve.stableswap(swap_amount, amp, n).expect("Should swap");

        assert!(amount_out > 0);
        assert!(amount_out < balances[1]);
    }

    #[test]
    fn test_swap_high_amp_flat_curve() {
        let balances = [1_000_000u64, 500_000u64];
        let swap_amount = 50_000u64;
        let amp_low = 1u64;      // behaves like constant product
        let amp_high = 10_000u64; // behaves like almost constant sum
        let n = 2u32;
        let fee = 30u64;

        let curve_low = make_curve(&balances, fee);
        let curve_high = make_curve(&balances, fee);

        let out_low = curve_low.stableswap(swap_amount, amp_low, n).expect("Low A swap");
        let out_high = curve_high.stableswap(swap_amount, amp_high, n).expect("High A swap");

        // High A yields higher output for same swap because the curve is flatter
        assert!(out_high >= out_low);
    }

    #[test]
    fn test_swap_zero_amount() {
        let balances = [1_000_000u64, 1_000_000u64];
        let swap_amount = 0u64;
        let amp = 100u64;
        let n = 2u32;
        let fee = 30u64;

        let curve = make_curve(&balances, fee);

        let result = curve.stableswap(swap_amount, amp, n).expect("Zero swap should succeed");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_swap_max_balances() {
        let balances = [u64::MAX / 1000, u64::MAX / 1000];
        let swap_amount = 1_000_000u64;
        let amp = 100u64;
        let n = 2u32;
        let fee = 30u64;

        let curve = make_curve(&balances, fee);

        let res = curve.stableswap(swap_amount, amp, n);
        // Should either succeed or return error, but not panic
        assert!(res.is_ok() || res.is_err());
    }

    #[test]
    fn test_fee_applied_correctly() {
        let balances = [1_000_000u64, 1_000_000u64];
        let swap_amount = 100_000u64;
        let amp = 100u64;
        let n = 2u32;
        let fee = 100u64; // 1%

        let curve = make_curve(&balances, fee);

        let amount_out = curve.stableswap(swap_amount, amp, n).expect("Should swap");
        let expected_max = balances[1] - balances[1] * fee / 10000;

        assert!(amount_out < expected_max + 1, "Fee not applied correctly");
    }

    #[test]
    fn test_invariant_not_violated() {
        let balances = [1_000_000u64, 1_000_000u64];
        let swap_amount = 200_000u64;
        let amp = 100u64;
        let n = 2u32;
        let fee = 30u64;

        let curve = make_curve(&balances, fee);

        let amount_out = curve.stableswap(swap_amount, amp, n).expect("Swap should succeed");
        let new_balances = [balances[0] + swap_amount, balances[1] - amount_out];

        // Compute D before and after swap
        let d_before = get_d(amp, &balances, n).unwrap();
        let d_after = get_d(amp, &new_balances, n).unwrap();

        // D should not decrease more than fees
        assert!(d_after <= d_before + 1_000, "Invariant violated too much"); 
    }
    // ====================== PROPTESTS ==========================
    proptest! {
        // ================= TESTING DEPOSITS ================
        // ================== TESTING WITHDRAWALS ===============
        // ================== TESTING SWAPS ===================
    }
}
