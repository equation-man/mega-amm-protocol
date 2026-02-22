//! Defining the protocol's errors.
use pinocchio::error::ProgramError as P;


// All fields will be of type u32
#[repr(u32)]
pub enum MegaAmmProgramError {
    InvalidInstructionData = 1,
    InvalidOwner = 2,
    Unauthorized = 3,
    InvalidAccountData = 4,
    InvalidAddress = 5,
    NotEnoughAccountKeys = 6,
    InvalidSignature = 7,
}

impl From<MegaAmmProgramError> for P {
    fn from(e: MegaAmmProgramError) -> Self {
        P::Custom(e as u32)
    }
}

impl From<P> for MegaAmmProgramError {
    fn from(e: P) -> Self {
        match e {
            P::AccountBorrowFailed => MegaAmmProgramError::Unauthorized,
            P::MissingRequiredSignature => MegaAmmProgramError::InvalidSignature,
            P::InvalidInstructionData => MegaAmmProgramError::InvalidInstructionData,
            _ => MegaAmmProgramError::InvalidAccountData,
        }
    }
}
