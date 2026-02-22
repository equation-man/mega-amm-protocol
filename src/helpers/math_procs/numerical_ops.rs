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
pub fn safeguarded_newton_solver(reserve: &[u128; 2], amp: u128) -> Result<u64, &'static str> {
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
    let mut sum_x = 0 as u128;
    for &x in reserve {
        sum_x = sum_x.checked_add(x).ok_or("Overflow addition")?;
    }
    let mut prod_x = 1 as u128;
    for &x in reserve {
        prod_x = prod_x.checked_mul(x).ok_or("Overflow multiplication")?;
    }
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

    // CONSTANT SUM TESTS
    #[test]
    fn test_constant_sum_basic() {
        let reserves = vec![100u128, 200u128, 300u128];
        let result = constant_sum(&reserves).unwrap();
        println!("Running the first constant product");
        assert_eq!(result, 600);
    }

    #[test]
    fn test_constant_sum_empty() {
        let reserves: Vec<u128> = vec![];
        let result = constant_sum(&reserves).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_constant_sum_overflow() {
        let reserves = vec![u128::MAX, 1u128];
        let result = constant_sum(&reserves);
        assert!(result.is_err());
    }

    // CONSTANT PRODUCT TESTS
    #[test]
    fn test_constant_product_basic() {
        let reserves = vec![10u128, 20u128];
        let result = constant_product(&reserves).unwrap();
        assert_eq!(result, 200);
    }

    #[test]
    fn test_constant_product_with_one() {
        let reserves = vec![5u128];
        let result = constant_product(&reserves).unwrap();
        assert_eq!(result, 5);
    }

    #[test]
    fn test_constant_product_overflow() {
        let reserves = vec![u128::MAX, 2u128];
        let result = constant_product(&reserves);
        assert!(result.is_err());
    }

    // TEST SLICE CONVERSION TO U128
    #[test]
    fn test_u128_conversion_success() {
        let input = vec![10u64, 20u64, 30u64];
        let mut output = vec![0u128; 3];

        u64_to_u128_inplace(&input, &mut output).unwrap();

        assert_eq!(output, vec![10u128, 20u128, 30u128]);
    }

    #[test]
    fn test_u128_conversion_small_output_slice() {
        let input = vec![10u64, 20u64];
        let mut output = vec![0u128; 1];

        let result = u64_to_u128_inplace(&input, &mut output);
        assert!(result.is_err());
    }

    // SAFEGUARDED NEWTON SOLVER TEST.
    #[test]
    fn test_newton_solver_two_equal_reserves() {
        let reserves = vec![1_000_000u128, 1_000_000u128];
        let amp = 100u128;

        let d = safeguarded_newton_solver(&reserves, amp).unwrap();

        // For symmetric pool, D ≈ sum
        assert!(d >= 1_999_000 && d <= 2_001_000);
    }

    #[test]
    fn test_newton_solver_imbalanced_pool() {
        let reserves = vec![2_000_000u128, 1_000_000u128];
        let amp = 100u128;

        let d = safeguarded_newton_solver(&reserves, amp).unwrap();

        // D must be between sum and n*max
        let sum = 3_000_000u128;
        let upper_bound = 2 * 2_000_000u128;

        assert!(d as u128 >= sum);
        assert!(d as u128 <= upper_bound);
    }

    #[test]
    fn test_newton_solver_zero_reserve_error() {
        let reserves: Vec<u128> = vec![];
        let amp = 100u128;

        let result = safeguarded_newton_solver(&reserves, amp);
        assert!(result.is_err());
    }
}
