use soroban_sdk::{contracterror, symbol_short, Env, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NftError {
    /// Contract has already been initialized
    AlreadyInitialized = 1,
    
    /// Contract has not been initialized
    NotInitialized = 2,
    
    /// Invalid input parameters
    InvalidInput = 3,
    
    /// Token does not exist
    TokenNotFound = 4,
    
    /// Caller is not authorized for this operation
    Unauthorized = 5,
    
    /// Operation not supported
    NotSupported = 6,
}

/// Error category for diagnostics
#[derive(Clone, Copy, Debug)]
pub enum ErrorCategory {
    /// Configuration error that needs admin intervention
    Configuration,
    /// User input error that can be corrected by caller
    UserInput,
    /// Permission error
    Authorization,
    /// Resource not found
    NotFound,
}

/// Comprehensive error information for debugging and client integration
#[derive(Clone, Debug)]
pub struct ErrorInfo {
    pub code: u32,
    pub category: ErrorCategory,
    pub description: &'static str,
    pub user_message: &'static str,
}

pub fn error_info(error: NftError) -> ErrorInfo {
    match error {
        NftError::AlreadyInitialized => ErrorInfo {
            code: 1,
            category: ErrorCategory::Configuration,
            description: "Contract has already been initialized and cannot be initialized again",
            user_message: "Contract already initialized",
        },
        NftError::NotInitialized => ErrorInfo {
            code: 2,
            category: ErrorCategory::Configuration,
            description: "Contract must be initialized before performing operations",
            user_message: "Contract not initialized",
        },
        NftError::InvalidInput => ErrorInfo {
            code: 3,
            category: ErrorCategory::UserInput,
            description: "Invalid input parameters provided",
            user_message: "Invalid input parameters",
        },
        NftError::TokenNotFound => ErrorInfo {
            code: 4,
            category: ErrorCategory::NotFound,
            description: "The specified token ID does not exist",
            user_message: "Token not found",
        },
        NftError::Unauthorized => ErrorInfo {
            code: 5,
            category: ErrorCategory::Authorization,
            description: "Caller is not authorized to perform this operation",
            user_message: "Unauthorized",
        },
        NftError::NotSupported => ErrorInfo {
            code: 6,
            category: ErrorCategory::UserInput,
            description: "Operation is not supported",
            user_message: "Operation not supported",
        },
    }
}

/// Helper function to emit error events for monitoring
pub fn emit_error_event(env: &Env, error: NftError, context: Option<Symbol>) {
    let info = error_info(error);
    let context = context.unwrap_or_else(|| symbol_short!("unknown"));
    
    env.events().publish(
        (symbol_short!("error"),),
        (info.code, context, Symbol::new(env, info.description)),
    );
}