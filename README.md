# Mega AMM protocol.
##### Technologies: Rust, Solana, Pinocchio  

## Overview:  
This is a high performance Automated Market Maker (AMM) supporting two-token liquidity pools with full 
lifecycle operations, that is, initialization, liquidity deposit, swaps and withdrawal.  
A swap fee of 10 bps is implemented to ensure liquidity providers are compensated for trades.  
The swap is based on constant product curve.  
Consider mathematical representation F(x, y) = k, where x is the reserve for token A and y is 
the reserve for token B, k is the invariant.  
The curve represents all possible (x, y) combinations that preseves or satisfies this invariant, hence, 
F(x, y) = k, for CPMM(constant product market maker) like this we have, x*y = k.   

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
Depositors can deposit supported token pairs to the pool and receive the LP tokens.  
- They enter amount of LP tokens they want.
- Specify maximum x for token A and y for token B, for slippage protection, and time bounds.
- Deposited Tokens are then transferred in exchange for the LP tokens for them.

#### Swap  
Users or traders swaps a token for another.  
- Here, the amount and the token to swap alongside the execution window is provided.
- The tokens are then swapped as necessary.  

#### Withdrawal  
Liquidity providers burn their LP tokens to redeem their share of the underlying assets.  
- They specify the amount of LP token to burn an the min bounds of the tokens to recieve.  
- Execution window is provided.  
