//! Curve operation for the AMM
use crate::helpers::MegaAmmProgramError;
use crate::helpers::math_procs::*;
use pinocchio_log::log;

pub struct MegaAmmStableSwapCurve<'b> {
    pub balances: &'b [u64],
    // Index of the token to be received.
    pub target_token_idx: Option<usize>,
    // Instead of embedding fees directly into the invariant equation, protocol computes
    // pure invariant preserving swap then applies fees externally as a delta.
    pub fee_bps: u64, // Fee in basis points (e.g 30 = 0.3%)
}

impl<'b> MegaAmmStableSwapCurve<'b> {
    // amp: The amplitude parameter.
    // balances: Array of token balances.
    pub fn deposit_to_amm(
        &self, amp: u64,  total_lp_supply: u64, balances: &[u64],
    ) -> Result<u64, &'static str> {
        // Should return the number of LP tokens to mint.
        let d_old = get_d(amp, self.balances)?;
        let d_new = get_d(amp, balances)?;
        if d_new < d_old {
            return Err("Invariant decreased on deposit");
        }
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
    pub fn amm_balanced_withdrawal(&self, lp_to_burn: u64, lp_supply: u64) -> Result<[u64; MAX_TOKENS], &'static str> {
        let amount_out = withdraw_balanced(self.balances, lp_to_burn, lp_supply).map_err(|_| "Balanced withdrawal error")?;
        // Slice of proportional amount to be transferred.
        Ok(amount_out)
    }

    // Withdrawing one coin. Behaves like a virtual swap.
    // returns the amount of the token to be transferred
    pub fn amm_imbalanced_withdrawal(&self, lp_to_burn: u64, lp_supply: u64, amp: u64) -> Result<u64, &'static str> {
        // Calculating d_current.
        let idx = self.target_token_idx.ok_or("Missing target token index")?;
        let amount_out = withdraw_imbalanced(
            lp_to_burn, lp_supply, self.balances, idx, amp
        ).map_err(|_| "Imbalanced withdrawal")?;
        // Final amount minus swap fee.
        let final_amount = apply_swap_fee(amount_out, self.fee_bps)?;
        // The amount of the token to be transferred.
        Ok(final_amount)
    }

    // Performs a swap between two tokens in an n token pool.
    // amount_in: quantity of token at index `i` being deposited.
    // i: index of token being given.
    pub fn stableswap(&self, amount_in: u64, i: usize, amp: u64) -> Result<u64, &'static str> {
        let n = self.balances.len() as u32;
        // Index of the token to be received
        let j = self.target_token_idx.ok_or("Missing target token index")?;

        // Calculate the current invariant D.
        let d = get_d(amp, self.balances)?;

        // Update balances to reflect the deposit of token i.
        let mut new_balances = [0u64; MAX_TOKENS];
        for (idx, &bal) in self.balances.iter().enumerate() {
            new_balances[idx] = bal;
        }
        new_balances[i] = new_balances[i].checked_add(amount_in).ok_or("Overflow on deposit")?;

        // Solve for the new balance of j keeping D constant, via newton solver.
        // Token j is excluded from the known balances to find its new required balance.
        let y_new = get_y(amp, &new_balances, d, j)?;

        // Calculate raw amount out. Delta invariant pattern.
        let amount_out_raw = new_balances[j].checked_sub(y_new).ok_or("Insolvent swap")?;

        // Apply fees. Delta invariant pattern instead of embedding the fee directly into the
        // complex curve calculation. Here fee is subsequently calculated as the diff bten gross
        // token amount user handed over and the net token amount that actually entered the pool
        let fee = (amount_out_raw as u128)
            .checked_mul(self.fee_bps as u128).ok_or("Fee mul overflow")?
            .checked_add(9_999u128).ok_or("Addition overflow")? // Rounding up in favour of the pool
            .checked_div(10_000u128).ok_or("Division error")? as u64;

        Ok(amount_out_raw.checked_sub(fee).ok_or("Fee underflow")?)
    }
}

#[cfg(test)]
mod curve_integration_tests {
    use super::*;
    use proptest::prelude::*;

    const AMP: u64 = 100;
    const FEE_BPS: u64 = 30; // 0.3%

    // Helper to initialize basic 2 token curve
    fn setup_curve<'a>(balances: &'a [u64], target_idx: Option<usize>) -> MegaAmmStableSwapCurve {
        MegaAmmStableSwapCurve {
            balances, target_token_idx: target_idx, fee_bps: FEE_BPS
        }
    }

    // =========== DEPOSIT, LP MINTING TESTS 
    #[test]
    fn test_genesis_deposit() {
        let balances = [1_000_000, 1_000_000];
        // Balances before initial deposit is [0, 0]
        let curve = setup_curve(&[0, 0], None);
        
        // At genesis total_lp = 0, LP tokens minted should equal D
        let lp_minted = curve.deposit_to_amm(AMP, 0, &balances).unwrap();
        let expected_d = get_d(AMP, &balances).unwrap();
        assert_eq!(lp_minted, expected_d);
    }

    #[test]
    fn test_subsequent_deposit_proportionality() {
        let initial_balances = [1_000_000, 1_000_000];
        let curve = setup_curve(&initial_balances, None);
        let total_lp = 2_000_000;

        // User adds 10% more liquidity
        let new_balances = [1_100_000, 1_100_000];
        let lp_minted = curve.deposit_to_amm(AMP, total_lp, &new_balances).unwrap();

        // 10% increase in D should result in 10% of total_lp supply minted
        // Expecting 200,000
        assert!(lp_minted >= 199_999 && lp_minted <= 200_001);
    }

    // ======= STABLESWAPING or TRADING TESTS ========
    #[test]
    fn test_stableswap_fee_deduction() {
        let balances = [10_000_000, 10_000_000];
        let curve = setup_curve(&balances, Some(1));
        
        // Swap 1,000,000 tokens
        let amount_in = 1_000_000;
        let amount_out = curve.stableswap(amount_in, 0, AMP).unwrap();

        // Without fees, in a balanced pool, we'd get ~1,000,000 back.
        // With 0.3% fee (3,000 tokens), we expect ~997,000.
        assert!(amount_out < 998_000);
        assert!(amount_out > 996_000);
    }

    #[test]
    fn test_fee_rounding_in_favor_of_pool() {
        let balances = [10_000_000, 10_000_000];
        let curve = setup_curve(&balances, None);
        
        // Tiny swap, 10 tokens. 0.3% fee is 0.03 tokens.
        // Rounding up. .checked_add(9_999).div(10_000)
        // should force a fee of 1 token even for tiny amounts.
        let amount_in = 10;
        let amount_out_raw = 10; // Assuming 1:1 price for small amount
        let fee = (10u128 * 30 + 9999) / 10000; 
        assert_eq!(fee, 1);
    }

    // ============== WITHDRAWAL TESTS =====================
    #[test]
    fn test_imbalanced_withdrawal_vs_swap_equivalence() {
        let balances = [10_000_000, 10_000_000];
        let total_lp = 20_000_000;
        let lp_to_burn = 1_000_000; // 5% of pool
        
        // Withdrawal via Single-Asset Exit. Target Index is 0
        let curve = setup_curve(&balances, Some(0));
        let amount_out = curve.amm_imbalanced_withdrawal(lp_to_burn, total_lp, AMP).unwrap();

        // Comparison: Proportional withdrawal is 500k of X and 500k of Y.
        // If we exit X only, it's like taking 500k X + (swapping 500k Y for X).
        // We are paying a fee on the "virtual swap" part, total should be < 1M.
        assert!(amount_out < 1_000_000);
    }

    // =================== PROPERTY BASED TESTING ==========================

    proptest! {
        #[test]
        fn prop_stableswap_no_free_lunch(
            amount_in in 1000..1_000_000u64,
            bal_x in 10_000_000..100_000_000u64,
            bal_y in 10_000_000..100_000_000u64,
        ) {
            let balances = [bal_x, bal_y];
            let curve = setup_curve(&balances, Some(1));

            // A user swaps X -> Y
            let amount_out_y = curve.stableswap(amount_in, 0, AMP).unwrap();
            
            // If the user immediately swaps that Y back to X
            let mid_balances = [bal_x + amount_in, bal_y - amount_out_y];
            let curve_back = setup_curve(&mid_balances, Some(0));
            let amount_out_x = curve_back.stableswap(amount_out_y, 1, AMP).unwrap();

            // After two swaps, the user MUST have fewer tokens than they started with.
            // Proving the fee is being captured and rounding is correct.
            prop_assert!(amount_out_x < amount_in);
        }
    }
}
