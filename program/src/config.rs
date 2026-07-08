//! Initialize the Config account and stores all the information needed
//! for the amm. Creates the mint_lp Mint account and assigns the
//! mint_authority to the config account.
use core::cell::Ref;
use solana_address;
use pinocchio::{
    AccountView, Address,
    ProgramResult,
};

use crate::helpers::errors::MegaAmmProgramError;

#[repr(C)]
pub struct Config {
    state: u8, // Tracks current status of the AMM. Eg, Uninitialized, etc.
    seed: [u8; 8], // Unique seed for the AMM enabling existence of different ones uniquely.
    authority: Address, // Administrative control over the AMM
    mint_x: Address, // Mint address for token X in the pool
    mint_y: Address, // Mint address for token Y in the pool
    fee: [u8; 2], // The swap fee.
    config_bump: [u8; 1], // PDA config account derivation bump seed.
}
#[repr(u8)]
pub enum AmmState {
    Uninitialized = 0u8,
    Initialized = 1u8,
    Disabled = 2u8,
    WithdrawOnly = 3u8,
}
impl From<AmmState> for u8 {
    fn from(state: AmmState) -> Self {
        state as u8
    }
}

impl Config {
    pub const LEN: usize = size_of::<Config>();

    // ====================== READING DATA ===========================
    #[inline(always)]
    pub fn load(account_info: &AccountView) -> Result<&Self, MegaAmmProgramError> {
        // Load the config account data.
        if account_info.data_len() != Self::LEN {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        }

        unsafe {
            if account_info.owner().ne(&Address::from(crate::ID)) {
                return Err(MegaAmmProgramError::InvalidAccountData.into());
            }
        }
        
        // Borrow is scoped and enforced
        let data = account_info.try_borrow()?; // solana_account_view::Ref<[u8]>
        let res = unsafe {
            &*(data.as_ptr() as *const Config)
        };
        Ok(&res)
    }

    #[inline(always)]
    pub fn load_unchecked(account_info: &AccountView) -> Result<&Self, MegaAmmProgramError> {
        if account_info.data_len() != Self::LEN {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        }
        unsafe {
            if account_info.owner() != &Address::from(crate::ID) {
                return Err(MegaAmmProgramError::InvalidAccountData.into());
            }
        }
        Ok(unsafe {
            Self::from_bytes_unchecked(
                account_info.borrow_unchecked(),
            )
        })
    }

    // Return Config from given bytes.
    #[inline(always)]
    pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
        &*(bytes.as_ptr() as *const Config)
    }

    // Getter methods for safe field access.
    #[inline(always)]
    pub fn state(&self) -> u8 { self.state }
    #[inline(always)]
    pub fn seed(&self) -> u64 { u64::from_le_bytes(self.seed) }
    #[inline(always)]
    pub fn authority(&self) -> &Address { &self.authority }
    #[inline(always)]
    pub fn mint_x(&self) -> &Address { &self.mint_x }
    #[inline(always)]
    pub fn mint_y(&self) -> &Address { &self.mint_y }
    #[inline(always)]
    pub fn fee(&self) -> u16 { u16::from_le_bytes(self.fee) }
    #[inline(always)]
    pub fn config_bump(&self) -> [u8; 1] { self.config_bump }

    // =========================== WRITING DATA ====================
    // Return mutable Config from given bytes.
    #[inline(always)]
    pub unsafe fn from_bytes_unchecked_mut(bytes: &mut [u8]) -> &mut Self {
        &mut *(bytes.as_mut_ptr() as *mut Config)
    }

    #[inline(always)]
    pub fn load_mut(account_info: &AccountView) -> Result<&mut Self, MegaAmmProgramError> {
        if account_info.data_len() != Self::LEN {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        }
        unsafe {
            if account_info.owner().ne(&Address::from(crate::ID)) {
                return Err(MegaAmmProgramError::InvalidAccountData.into());
            }
        }
        let mut data = account_info.try_borrow_mut()?;
        // Converting RefMut<[u8]> to &mut [u8]
        let unsafe_data = unsafe {
            &mut *(data.as_mut_ptr() as *mut Config)
        };

        Ok(unsafe_data)
    }

    #[inline(always)]
    pub fn set_state(&mut self, state: u8) -> Result<(), MegaAmmProgramError> {
        if state == (AmmState::WithdrawOnly as u8) {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        }
        self.state = state as u8;
        Ok(())
    }

    #[inline(always)]
    pub fn set_fee(&mut self, fee: u16) -> Result<(), MegaAmmProgramError> {
        if fee.ge(&10_000) {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        }
        self.fee = fee.to_le_bytes();
        Ok(())
    }

    #[inline(always)]
    pub fn set_inner(
        &mut self, seed: u64, authority: [u8; 32],
        mint_x: [u8; 32], mint_y: [u8; 32], fee: u16,
        config_bump: [u8; 1],
    ) -> Result<(), MegaAmmProgramError> {
        self.set_state(AmmState::Initialized as u8)?;
        self.set_seed(seed);
        self.set_authority(authority);
        self.set_mint_x(mint_x);
        self.set_mint_y(mint_y);
        self.set_fee(fee)?;
        self.set_config_bump(config_bump);
        Ok(())
    }

    #[inline(always)]
    pub fn set_seed(&mut self, seed: u64) -> Result<(), MegaAmmProgramError> {
        self.seed = seed.to_le_bytes();
        Ok(())
    }

    #[inline(always)]
    pub fn set_authority(&mut self, authority: [u8; 32]) -> Result<(), MegaAmmProgramError> {
        self.authority = authority.into();
        Ok(())
    }

    #[inline(always)]
    pub fn set_mint_x(&mut self, mint_x: [u8; 32]) -> Result<(), MegaAmmProgramError> {
        self.mint_x = mint_x.into();
        Ok(())
    }

    #[inline(always)]
    pub fn set_mint_y(&mut self, mint_y: [u8; 32]) -> Result<(), MegaAmmProgramError> {
        self.mint_y = mint_y.into();
        Ok(())
    }

    #[inline(always)]
    pub fn set_config_bump(&mut self, config_bump: [u8; 1]) -> Result<(), MegaAmmProgramError> {
        self.config_bump = config_bump;
        Ok(())
    }

    #[inline(always)]
    pub fn has_authority(&self) -> Option<Address> {
        let bytes = self.authority.as_ref();
        let chunks: &[u64; 4] = unsafe { &*(bytes.as_ptr() as *const [u64; 4]) };
        if chunks.iter().any(|&x| x != 0) {
            Some(self.authority.clone()) // Cloning an address is cheap here
        } else {
            None
        }
    }

}
