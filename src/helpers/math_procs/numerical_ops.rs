//! utility functions for performing price discovery math.
use core::cmp::Ordering;
use pinocchio_log::log;

type Uint = u128; // Used to represent fixed point numbers (1e18 decimals).
pub const MAX_TOKENS: usize = 2;

// Fee calculator function. Uses ceiling division
pub fn apply_swap_fee(
    amount_out_raw: u64,
    fee_bps: u64, // e.g., 30 for 0.3%
) -> Result<u64, &'static str> {
    if fee_bps == 0 {
        return Ok(amount_out_raw);
    }

    // Calculating fee using u128 to prevent overflow
    // Formula: (Amount * FeeBps) / 10,000. 10,000 Bps equals 100%
    let fee = (amount_out_raw as u128)
        .checked_mul(fee_bps as u128)
        .ok_or("Fee multiplication overflow")?
        .checked_div(10_000)
        .ok_or("Fee division error")?;

    // Subtract fee from the raw amount
    let final_amount = (amount_out_raw as u128)
        .checked_sub(fee)
        .ok_or("Fee underflow")?;

    Ok(final_amount as u64)
}

// Calculating the invariant D using Newton's method.
// This computes the pool's total virtual liquidity surface
// Invariant equation is Ann * sum(x_i) + D = Ann * D + D^(n+1) / (n^n * prod(x_i))
pub fn get_d(amp: u64, balances: &[u64]) -> Result<u64, &'static str> {
    let n_len = balances.len();
    // Scaling to u128 to accomodate large integer computations.
    let n = n_len as Uint;
    let sum_x: Uint = balances.iter().map(|&x| x as Uint).sum();
    if sum_x == 0 { return Ok(0); }

    let mut d = sum_x;
    let ann = if n_len.is_power_of_two() {
        let k = n_len.trailing_zeros() as Uint;
        let nn = 1u128 << (k.checked_mul(n).ok_or("Bitshift overflow")?);
        (amp as Uint).checked_mul(nn).ok_or("Ann overflow")?
    } else {
        (amp as Uint)
            .checked_mul(n.checked_pow(n_len as u32).ok_or("Power overflow")?).ok_or("Ann overflow")?
    };

    for _ in 0..32 {
        let mut d_p = d;
        for &x in balances {
            if x == 0 {
                return Err("Zero balance in invariant");
            }

            let x_u128 = x as Uint;
            let denom = x_u128.checked_mul(n).ok_or("Overflow")?;
            // d_p = d_p * d / (x*n)
            d_p = d_p.checked_mul(d).ok_or("D_p mul overflow")?
                .checked_div(denom)
                .ok_or("D_p div error")?;
        }

        // Convergence check
        let d_prev = d;

        // Newton's method for d.
        // d = [ (Ann * sum_x + d_p *n) * d ] / [ (Ann - 1) * d + (n+1) * d_p ]
        let num = d.checked_mul(ann.checked_mul(sum_x).ok_or("Overflow error on computing numerator")?
            .checked_add(d_p.checked_mul(n).ok_or("Overflow error on computing numerator")?)
            .ok_or("Overflow error on addition in numerator")?)
            .ok_or("Overflow on D computation")?;
        
        let den = d.checked_mul(ann.checked_sub(1).ok_or("Underflow error for denominator")?)
            .ok_or("Overflow on denominator computation")?
            .checked_add(d_p.checked_mul(n.checked_add(1).ok_or("Overflow on denominator addition")?)
                .ok_or("Overflow on denominator multiplication")?)
            .ok_or("Overflow on addition on the denominator")?;

        d = num.checked_div(den).ok_or("D div error")?;

        // Checking convergence.
        if d > d_prev && d - d_prev <= 1 { return Ok(d.try_into().map_err(|_| "Error scalling down to u64")?); }
        if d_prev > d && d_prev - d <= 1 { return Ok(d.try_into().map_err(|_| "Error scalling down to u64")?); }
    }
    // Return new value of the liquidity after deposit. Will be used to calculate LP tokens to mint
    Ok(d.try_into().map_err(|_| "Error scalling down deposit")?)
}

// Withdrawal function to withdraw one coin.
// Here, we will use our Newton solver as this is treated as a virtual swap.
pub fn withdraw_imbalanced(
    lp_tokens_to_burn: u64, 
    total_lp_supply: u64,
    current_balances: &[u64],
    target_token_index: usize,
    amp: u64
) -> Result<u64, &'static str> {
    if lp_tokens_to_burn == 0 || total_lp_supply == 0 {
        return Ok(0);
    }
    // Calculating the current liquidity D.
    let d_current = get_d(amp, current_balances)?;
    // Calcuating the invariant after withdrawal to find new value of y.
    let d_reduction = d_current.checked_mul(lp_tokens_to_burn).ok_or("Multiplication overflow")?
        .checked_div(total_lp_supply).ok_or("Division by zero")?;
    let d_target = d_current.checked_sub(d_reduction).ok_or("D underflow")?;

    // Finding the new balance for token y.
    let y_new = get_y(
        amp,
        current_balances, 
        d_target,
        target_token_index
    )?;
    
    // Extracting the old balance for token y
    let old_balance = current_balances[target_token_index] as Uint;

    let amount_out_raw = old_balance.checked_sub(y_new as Uint).ok_or("Withdrawal limit exceeded")?;

    // Applying fee. Fee is applied where this function is called.
    //let fee = amount_out_raw.checked_div(100).unwrap_or(0); //0.1% fee
    //let final_payment = amount_out_raw.checked_sub(fee).ok_or("Fee error")?;
    Ok(amount_out_raw.try_into().map_err(|_| "Error scaling down withraw result")?)
}

// Withdrawing proportional amount of each token from the pool.
pub fn withdraw_balanced(reserves: &[u64], lp_to_burn: u64, total_lp_supply: u64) -> Result<[u64; MAX_TOKENS], &'static str> {
    let n_len = reserves.len();
    if n_len == 0 || n_len > MAX_TOKENS { return Err("Invalid reserve length"); }
    if total_lp_supply == 0 { return Err("Zero total supply"); }
    if lp_to_burn == 0 { return Ok([0u64; MAX_TOKENS]); }
    if lp_to_burn > total_lp_supply { return Err("Burn amount exceeded supply"); }

    let mut amount_out = [0u64; MAX_TOKENS];
    for i in 0..n_len {
        let reserve_i: Uint = reserves[i].into();
        let burn_amt: Uint = lp_to_burn.into();
        let supply: Uint = total_lp_supply.into();

        let out = reserve_i.checked_mul(burn_amt).ok_or("Overflowing balanced withdrawal")?
            .checked_div(supply).ok_or("Div error for withdrawal")?;

        amount_out[i] = out.try_into().map_err(|_| "Error scaling down balanced withdrawal")?;
    }
    // Return array containing amount of each token to be sent to the user
    // returns them with the arrangement that they were supplied
    Ok(amount_out)
}

// Newton-Raphson(NR) with Bisection fallback to solve for token y.
// Formula for next approximation is: y_next = y_current - (f(y_current)/f'(y_current))
// We rearrange this formula to a form that minimizes the risk of overflow and -ve nos.
// For finding a single token y, keeping D and all other balances x constant, it can be 
// reduced to quadratic style equation y^2 + y(b-D) - c = 0
// where b is the sum term sum(x_others) + D/An^n and c is the product term.
// D^(n-1)/(An^n*n^n * prod(x_others))
// We perform algebraic manipulation to eliminate the negative since direct computation
// would make (b-D) cause crash due to negatives. Hence we will have
// y_next = (y^2 + c)/(2y + b - D) that we solve for as our newton approx formula.
// amp: Amplification coefficient A.
// balances: Current balances including the updated input token.
// d: Current pool invariant(taget liquidity).
// j: Index of the token we are solving for.
#[inline(always)]
pub fn get_y(
    amp: u64, balances: &[u64], d: u64, j: usize
) -> Result<u64, &'static str> {
    // No tokens available in in the pool yet
    if balances.len() == 0 {
        return Err("Insufficient funds");
    }
    // Scaling up to u128 to accommodate large integer computations.
    let n_len = balances.len(); // Number of tokens
    let n = n_len as Uint;
    let d_u128 = d as Uint;
    // Computing A*n^n. Optimized for bitwise powers of two with bit shifts.
    // If n=2^k, then n^n = 2^(k*2^k).
    let ann = if n_len.is_power_of_two() {
        let k = n_len.trailing_zeros() as Uint; // Finding k where n = 2^k
        // Shifting left by k*n to get n^n. Shifting left is multiplying by powers of 2.
        let nn = 1u128 << (k.checked_mul(n).ok_or("Bitshift overflow")?); // n^n = 2^(k*n)
        (amp as Uint).checked_mul(nn).ok_or("Ann overflow")?
    } else {
        (amp as Uint)
            .checked_mul(n.checked_pow(n_len as u32).ok_or("Power overflow")?).ok_or("Ann overflow")?
    };

    let mut s_prime = 0 as Uint;
    let mut c = d_u128;

    // Solving for constants b & c. Iterate through all tokens except one we are solving for.
    for (idx, &x) in balances.iter().enumerate() {
        if idx != j {
            let x_u128 = x as Uint;
            // Sum of all tokens except the output token
            s_prime = s_prime.checked_add(x_u128).ok_or("Sum overflow")?;

            // c = C * D / (x * n)
            // Building the term D^(n+1) / (n^n * prod(x_others))
            c = c.checked_mul(d_u128).ok_or("C multiplication overflow")?
                .checked_div(x_u128.checked_mul(n).ok_or("Division overflow")?)
                .ok_or("C division error")?;
        }
    }
    c = c.checked_mul(d_u128).ok_or("C overflow")?
        .checked_div(ann.checked_mul(n).ok_or("C mul error 2")?)
        .ok_or("C division error 2")?;
    let b = s_prime.checked_add(d_u128.checked_div(ann).ok_or("b div error")?).ok_or("B overflow")?;

    // Better initial guess with Bitwise shift. y=D not taken directly but y=D - S' which is much
    // closer to the root.
    let mut y = d_u128.checked_sub(s_prime).unwrap_or(d_u128);

    // Bounds for hybrid bisection. Safeguarding Newton.
    let mut y_min = 0u128;
    let mut y_max = d_u128 << 1; // Bitwise shift for D * 2

    for _ in 0..32 {
        let y_prev = y;

        // BITWISE NEWTON STEP
        // We are computing y_next = (y^2 + c) / (2y + b - D)
        let num = y.checked_mul(y).ok_or("y_sq overflow")?
            .checked_add(c).ok_or("num overflow")?; // y^2 + c
        // Use `y << 1` instead of `y * 2`.
        let den = (y << 1).checked_add(b).ok_or("den_add overflow")?
            .checked_sub(d_u128).ok_or("den_sub underflow")?;
        // We can't avoid division. Bisection fallback with shifts. We use ceiling division
        // to help round in favour of the pool.
        // (a / b) rounding up will be (a + b - 1) / b
        let y_next = num.checked_add(den.checked_sub(1).unwrap_or(0))
            .ok_or("Overflow in rounding up")?
            .checked_div(den).unwrap_or_else(|| (y_min + y_max) >> 1);

        // BITWISE CONVERGENCE CHECK
        // Checking if diff <= 1.
        let diff = if y_next > y { y_next - y } else { y - y_next };
        if diff <= 1 { return Ok(y_next as u64); }

        // BOUND ENFORCEMENT WITH SHIFTED MIDPOINT.
        if y_next > y_min && y_next < y_max {
            y = y_next;
        } else {
            // Midpoint calc using shift: (min + max) >> 1
            y = (y_min + y_max) >> 1;
        }

        // Update search range for Bisection safety
        if y > y_prev { y_min = y_prev; } else { y_max = y_prev; }
    }

    Ok(y.try_into().map_err(|_| "Error scaling down")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    type Uint = u128;

    // ================== Invariant Verification ==================
    fn check_d_consistency(amp: u64, balances: &[u64], d_expected: u64) {
        let d_actual = get_d(amp, balances).expect("D calculation failed");
        let diff = if d_actual > d_expected { d_actual - d_expected } else {d_expected - d_actual};
        // Allowing tolerance of 1 due to integer truncation.
        assert!(diff <= 1, "Invariant mismatch: Expected {}, got {}", d_expected, d_actual);
    }

    // ================ BASIC STABILITY TESTS =======================
    #[test]
    fn test_get_d_balanced_pool() {
        let amp = 100;
        let balances = [1_000_000, 1_000_000]; // Stable pool, perfectly balanced
        let d = get_d(amp, &balances).unwrap();

        // D should be exactly the same as the sum of the balances.
        assert_eq!(d, 2_000_000);
    }

    #[test]
    fn test_get_y_balanced_pool() {
        let amp = 100;
        let balances = [1_000_000, 1_000_000];
        let d = 2_000_000;

        // Solving for token at index 0 given other is 1M and D is 2M
        let y = get_y(amp, &balances, d, 0).unwrap();
        assert!(y >= 999_999 && y <= 1_000_001);
    }

    // =================== SYMMETRY AND ECONOMIC TESTS =====================
    #[test]
    fn test_swap_symmetry() {
        let amp = 85;
        let initial_balances = [10_000_000, 10_000_000];
        let d = get_d(amp, &initial_balances).unwrap();

        // User deposits 1,000,000 of Token 0
        let new_x = 11_000_000;
        let mid_balances = [new_x, 10_000_000];
        // Solving how much token 1 remains.
        let new_y = get_y(amp, &mid_balances, d, 1).unwrap();

        // D should remain consistent if we use the new [x, y] for calculation.
        let final_balances = [new_x, new_y];
        check_d_consistency(amp, &final_balances, d);
    }

    #[test]
    fn test_extreme_imbalance_convergence() {
        let amp = 100;
        let balances = [10_000_000_000, 1_000]; // Extreme Imbalance
        let d = get_d(amp, &balances);
        assert!(d.is_ok(), "Failed to converge on extreme imbalance");
    }


    // ==================== PROPERTY TESTING FOR ROBUSTNESS =======================
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]
        #[test]
        fn prop_invariant_must_not_decrease_after_swap(
            amp in 10..5000u64, bal_a in 100_000..100_000_000u64,
            bal_b in 100_000..100_000_000u64, swap_amount in 1..50_000u64
        ) {
            let initial_balances = [bal_a, bal_b];
            let d_initial = get_d(amp, &initial_balances).unwrap();

            // Simulate swap: Add to A, solve for B.
            let updated_balances = [bal_a + swap_amount, bal_b];
            let y = get_y(amp, &updated_balances, d_initial, 1).unwrap();

            let final_balances = [bal_a + swap_amount, y];
            let d_final = get_d(amp, &final_balances).unwrap();

            // Economic security. D must never shrink to prevend draining of the pool.
            // It can stay same or grow slightly due to rounding.
            prop_assert!(d_final >= d_initial - 1);
        }
    }

    // ============== TESTING WITHDRAWALS ==================================o
    #[test]
    fn test_withdraw_balanced_scenario() {
        let amp = 100;
        let balances = [1_000_000, 1_000_000]; // D = 2,000_000
        let total_lp = 2_000_000;
        let burn_amount = 200_000; // 10% of the pool

        // If 10% of the pool is withdrawn in a balanced state
        // we should get roughly 10% of the total liquidity that is 200k tokens
        let amount_out = withdraw_imbalanced(
            burn_amount, total_lp, &balances,
            0, // Target token X
            amp
        ).unwrap();

        // We expect  ~200,000, but not exact. 
        // StableSwap math actually gives a tiny "bonus" or "penalty" 
        // even in balanced pools for single sided exits compared to multi sided.
        assert!(amount_out > 190_000 && amount_out < 210_000);
    }

    #[test]
    fn test_withdraw_full_exit() {
        let amp = 100;
        let balances = [1_000_000, 1_000_000];
        let total_lp = 2_000_000;
        
        // Burning 100% of the supply
        let amount_out = withdraw_imbalanced(
            total_lp, total_lp, &balances,
            1, // Target token Y
            amp
        ).unwrap();

        // Should return the entire balance of that token
        assert_eq!(amount_out, 1_000_000);
    }

    // ==== economic properties, rebalancing bonus ======= 
    #[test]
    fn test_rebalancing_efficiency() {
        let amp = 100;
        // Pool is heavy in Token X (1.5M) and light in Token Y (0.5M). Total D ~ 2M.
        let balances = [1_500_000, 500_000];
        let total_lp = 2_000_000;
        let burn_amount = 100_000;

        // Withdrawing the Over-supplied token (X). This helps the pool.
        let out_x = withdraw_imbalanced(burn_amount, total_lp, &balances, 0, amp).unwrap();

        // Withdrawing the Under-supplied token (Y). This hurts the pool.
        let out_y = withdraw_imbalanced(burn_amount, total_lp, &balances, 1, amp).unwrap();

        // Economic Law: You should get MORE tokens when you help rebalance the pool.
        assert!(out_x > out_y, "Rebalancing bonus failed: out_x ({}) should be > out_y ({})", out_x, out_y);
    }

    // ================= PROPERTY-BASED TESTING
    proptest! {
        #[test]
        fn prop_withdraw_never_exceeds_total_balance(
            amp in 10..2000u64,
            bal_x in 1_000_000..10_000_000u64,
            bal_y in 1_000_000..10_000_000u64,
            burn_percent in 1..99u64,
        ) {
            let balances = [bal_x, bal_y];
            let total_lp = get_d(amp, &balances).unwrap();
            let burn_amount = (total_lp * burn_percent) / 100;

            let out_x = withdraw_imbalanced(burn_amount, total_lp, &balances, 0, amp).unwrap();
            let out_y = withdraw_imbalanced(burn_amount, total_lp, &balances, 1, amp).unwrap();

            // Safety: Can never withdraw more than the pool has
            prop_assert!(out_x <= bal_x);
            prop_assert!(out_y <= bal_y);
        }

        #[test]
        fn prop_d_reduction_is_proportional(
            amp in 10..1000u64,
            bal_x in 1_000_000..5_000_000u64,
            bal_y in 1_000_000..5_000_000u64,
            burn_amount in 100_000..500_000u64,
        ) {
            let balances = [bal_x, bal_y];
            let d_initial = get_d(amp, &balances).unwrap();
            let total_lp = d_initial; // 1 D = 1 LP token

            let amount_out = withdraw_imbalanced(burn_amount, total_lp, &balances, 0, amp).unwrap();
            
            // New state after user takes their tokens
            let new_balances = [bal_x - amount_out, bal_y];
            let d_final = get_d(amp, &new_balances).unwrap();

            // The reduction in D should match the proportion of LP tokens burned
            let d_reduction_actual = d_initial - d_final;
            let d_reduction_expected = (d_initial as u128 * burn_amount as u128 / total_lp as u128) as u64;

            // Allow for tiny rounding difference
            let diff = if d_reduction_actual > d_reduction_expected {
                d_reduction_actual - d_reduction_expected
            } else {
                d_reduction_expected - d_reduction_actual
            };
            
            prop_assert!(diff <= 2);
        }
    }
}

