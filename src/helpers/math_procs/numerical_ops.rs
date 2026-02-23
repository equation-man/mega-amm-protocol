//! utility functions for performing price discovery math.

// Calculating the contant product, sum of x_i.
pub fn constant_sum(reserves: &[u128]) -> Result<u128, &'static str> {
    reserves.iter().try_fold(0u128, |acc, &x| {
        acc.checked_add(x as u128).ok_or("Constant sum overflow")
    })
}

// Calculating the constant product, product of x_i.
pub fn constant_product(reserves: &[u128]) -> Result<u128, &'static str> {
    reserves.iter().try_fold(1u128, |acc, &x| {
        acc.checked_mul(x as u128).ok_or("Constant product overflow")
    })
}

// Converting &[u64] to &[u128] for scaling pattern in arithmetic computation.
pub fn u64_to_u128_inplace(
    reserve: &[u64], out: &mut [u128; 2]
) -> Result<(), &'static str> {
    if out.len() < reserve.len() { return Err("Small output slice"); }

    reserve.iter()
        .zip(out.iter_mut())
        .for_each(|(&x, new_type)| *new_type = u128::from(x));
    Ok(())
}

// Newton-Raphson(NR) with Bisection fallback
// Here we are solving for the invariant "D" from formula
// f(D) = (an^n -1)D + D^(n+1)/n^n(prod(x_i)) - an^n(sum(x_i)),
// We'll also need first derivatite for this formula to perform NR process.
// xi represent the token balances in the pool or reserves.
// We use "Scaling" pattern, working with u128 to prevent potential overflow and convert back to 
// u64 for storage.
pub fn safeguarded_newton_solver(reserve: &[u128], amp: u128) -> Result<u64, &'static str> {
    if reserve.is_empty() {
        return Err("Zero division error");
    }
    //n and n^n 
    let n = reserve.len() as u128;
    let mut n_pow_n = 1 as u128;
    for _ in 0..n {
        n_pow_n = n_pow_n.checked_mul(n).ok_or("Overflow multiplication")?;
    }

    // A(amplifier) * n^n
    let ann = amp.checked_mul(n_pow_n).ok_or("Overflow multiplication")?;

    // sum of xi and product of xi
    let sum_x = constant_sum(reserve)?;
    let prod_x = constant_product(reserve)?;

    let n_pow_n_prod_x = n_pow_n.checked_mul(prod_x).ok_or("Overflow multiplication")?;

    // Specifying the bounds for the Newton-Raphson process.
    // The process will be contained inside these bounds
    let max_x = *reserve.iter().max_by(|a, b| a.cmp(b)).ok_or("Zero division error")?;
    let mut low = sum_x;
    let mut high = max_x.checked_mul(n).ok_or("Overflow multiplication")?;

    let mut d = sum_x; // This is now the initial guess or value of d

    // Iteratively probing the value of D using Newton-Raphson process.
    for _ in 0..20 {
        // D^n
        let mut d_pow_n = 1 as u128;
        for _ in 0..n {
            d_pow_n = d_pow_n.checked_mul(d).ok_or("Overflow multiplication")?;
        }
        // D^(n+1)
        let d_pow_n_plus_1 = d_pow_n.checked_mul(d).ok_or("Overflow multiplication")?;

        let term_a = ann.checked_sub(1)
            .ok_or("Subraction error")?
            .checked_mul(d).ok_or("Overflow multiplication")?;
        let term_b = d_pow_n_plus_1.checked_div(n_pow_n_prod_x).ok_or("Division error")?;
        let term_c = ann.checked_mul(sum_x).ok_or("Overflow multiplication")?;
        // f(D)
        let f_d = term_a.checked_add(term_b)
            .ok_or("Overflowing addition")?
            .checked_sub(term_c).ok_or("Overflowing subtraction")?;
        // f'(D) calculating for the first derivative.
        let df_term_b = (n + 1).checked_mul(d_pow_n)
            .ok_or("Overflowing multiplication")?
            .checked_div(n_pow_n_prod_x).ok_or("Division error")?;
        let df_d = ann.checked_sub(1).ok_or("Subtraction error")?
            .checked_add(df_term_b).ok_or("Addition error")?;

        // Newton step
        let next_d = if df_d > 0 {
            d.checked_sub(f_d.checked_div(df_d)
                .ok_or("Division error")?).ok_or("Subraction error")?
        } else {
            d
        };

        // Bisection fallback, if we're out of bounds, to adjust to average.
        let next_d = if next_d <= low || next_d >= high {
            low.checked_add(high).ok_or("Addition error")?
                .checked_div(2).ok_or("Division error")?
        } else {
            next_d
        };

        // Updating the bounds.
        if f_d > 0 { high = d; } else { low =d; }

        // If we reach convergence point.
        let diff = if d > next_d { 
            d.checked_sub(next_d).ok_or("Subtraction error")?
        } else {
            next_d.checked_sub(d).ok_or("Subtraction error")?
        };
        if diff <= 1 {
            return Ok(next_d.try_into().expect("Value out of range"));
        }
        d = next_d;
    }

    Ok(d.try_into().expect("Value out of range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Testin f(D) 
    fn evaluate_f(
        reserve: &[u128],
        amp: u128,
        d: u128,
    ) -> u128 {

        let n = reserve.len() as u128;

        let mut n_pow_n = 1u128;
        for _ in 0..n {
            n_pow_n *= n;
        }

        let ann = amp * n_pow_n;

        let sum_x = constant_sum(reserve).unwrap();
        let prod_x = constant_product(reserve).unwrap();

        let mut d_pow_n = 1u128;
        for _ in 0..n {
            d_pow_n *= d;
        }

        let d_pow_n_plus_1 = d_pow_n * d;

        let term_a = (ann - 1) * d;
        let term_b = d_pow_n_plus_1 / (n_pow_n * prod_x);
        let term_c = ann * sum_x;

        term_a + term_b - term_c
    }

    // Balanced pool test
    #[test]
    fn test_balanced_pool_converges() {
        let reserve = [1_000_000u128, 1_000_000u128];
        let amp = 100u128;

        let d = safeguarded_newton_solver(&reserve, amp).unwrap();

        // For perfectly balanced pool, D should be ~ sum_x
        assert!(d >= 2_000_000 - 2);
        assert!(d <= 2_000_000 + 2);

        // Verify f(D) = 0
        let f_val = evaluate_f(&reserve, amp, d as u128);
        assert!(f_val <= 2);
    }

    // Imbalanced pool test
    #[test]
    fn test_imbalanced_pool_converges() {
        let reserve = [1_000_000u128, 100_000u128];
        let amp = 100u128;

        let d = safeguarded_newton_solver(&reserve, amp).unwrap();

        let sum_x = constant_sum(&reserve).unwrap();

        // D should always be >= sum_x
        // This is economic property check
        assert!(d as u128 >= sum_x);

        // Check invariant condition
        let f_val = evaluate_f(&reserve, amp, d as u128);
        println!("The convergence at test imbalance {}", f_val);
        assert!(f_val < 2);
    }

    // Very high amplification (stable-swap behavior)
    #[test]
    fn test_high_amp_behaves_like_constant_sum() {
        let reserve = [5_000_000u128, 5_000_000u128];
        let amp = 10_000u128;

        let d = safeguarded_newton_solver(&reserve, amp).unwrap();

        // With high A, D = sum
        assert!((d as i128 - 10_000_000i128).abs() <= 2);
    }

    // Very low amplification (approaches constant product)
    #[test]
    fn test_low_amp_behaves_like_constant_product() {
        let reserve = [1_000_000u128, 500_000u128];
        let amp = 1u128;

        let d = safeguarded_newton_solver(&reserve, amp).unwrap();

        let sum_x = constant_sum(&reserve).unwrap();
        assert!(d as u128 >= sum_x);
    }

    // Zero reserve should fail
    #[test]
    fn test_zero_reserve_fails() {
        let reserve: [u128; 0] = [];
        let amp = 100u128;

        let result = safeguarded_newton_solver(&reserve, amp);
        assert!(result.is_err());
    }

    // Convergence robustness across ranges
    #[test]
    fn test_multiple_random_like_values() {
        let test_cases = vec![
            [10u128, 20u128],
            [1_000u128, 3_000u128],
            [100_000u128, 200_000u128],
            [999_999u128, 123_456u128],
        ];

        for reserve in test_cases {
            let amp = 100u128;

            let d = safeguarded_newton_solver(&reserve, amp).unwrap();

            let sum_x = constant_sum(&reserve).unwrap();

            // D should never be smaller than sum
            assert!(d as u128 >= sum_x);

            let f_val = evaluate_f(&reserve, amp, d as u128);
            println!("The convergence at multiple random values {}", f_val);

            // Should converge very close to zero
            assert!(f_val < 2);
        }
    }

    // Large value stress test
    #[test]
    fn test_large_values() {
        let reserve = [
            1_000_000_000_000u128,
            1_000_000_000_000u128
        ];
        let amp = 100u128;

        let d = safeguarded_newton_solver(&reserve, amp).unwrap();

        assert!(d > 0);
    }
}
