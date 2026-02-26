# Mega AMM protocol.
##### Technologies: Rust, Solana, Pinocchio  
[*Inside DeFi protocols beyond the AMM curves. Engineering StableSwap invariant*](https://medium.com/@EquationManTheBlueBeetle/inside-defi-protocols-beyond-the-amm-curves-engineering-the-stableswap-invariant-in-rust-560eb8b21706)  

## Overview:  
This is a high performance Automated Market Maker (AMM) supporting two-token liquidity pools with full 
lifecycle operations, that is, initialization, liquidity deposit, swaps and withdrawal.  
A swap fee of 10 bps is implemented to ensure liquidity providers are compensated for trades.  
The swap is based on constant product curve.  
Consider mathematical representation F(x, y) = k, where x is the reserve for token A and y is 
the reserve for token B, k is the invariant.  
The curve represents all possible (x, y) combinations that preseves or satisfies this invariant, hence, 
F(x, y) = k, for CPMM(constant product market maker) like this we have, x*y = k.   
You can find more about this in the medium article [Inside DeFi protocols beyond the AMM curves. Engineering StableSwap invariant](https://medium.com/@EquationManTheBlueBeetle/inside-defi-protocols-beyond-the-amm-curves-engineering-the-stableswap-invariant-in-rust-560eb8b21706)  
  

## Features:  
- Permissionless pool creation. Anyone can create liquidity in the pool without any approval.
- LP token minting. Depositors receive LP tokens that represent their share of the pool.
- Swapping token pairs. Efficient on-chain swaps between tokens with autormatic fee deduction.
- Withdrawals. Liquidity providers can burn tokens and redeem the underlying assets.
- Slippage checks for deposits and withdrawals to protect against slippage.
- MEV mitigation, enforced by bounded execution windows, or times to manage front-running and sandwich attacks.  

## How it works:  
#### Initialization  
The AMM is initialized with necessary configurations, and the pool, which belongs to the config pda that signs on behalf of the 
program.

#### Deposit  
Here the system solves for invariant new D using Newton solver.  
- User provides balances, [x1, x2, ...] (e.g 100USDC and 100 USDT).
- Newton solver iterates until it finds unique D(liquidity) that satifies the equation.
- The protocol issues LP tokens to the user proportional tho how much D increased.

#### Swap  
Users or traders swaps a token for another.  
- Here, the amount and the token to swap alongside the execution window is provided.
- The tokens are then swapped as necessary.  

#### Withdrawal  
There are two types of withdrawals. A balanced withdrawal where no solver is required, and 
an imbalanced withdrawal where a solver is required as it performs a virtual swap.  
- In a balanced withdrawal, if a user wants to withdraw a percentage of their LP tokens, the protocol gives them 10% of every token in the pool.
- In imbalanced withdrawal, the protocol calculates the invariant after removing user's share value.
- Protocol uses Newton solver to find how much USCDC must remain in the pool to satify the target D(liquidity).
- The difference is sent to the user.
