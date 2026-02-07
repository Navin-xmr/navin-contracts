use crate::types::*;
use soroban_sdk::{Address, Env, Vec};

/// Log a transaction in the vault's history
pub fn log_transaction(
    env: &Env,
    from: &Address,
    to: &Address,
    amount: i128,
    transaction_type: TransactionType,
) {
    let transaction = TransactionLog {
        from: from.clone(),
        to: to.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
        transaction_type,
    };

    let mut transactions: Vec<TransactionLog> = env
        .storage()
        .instance()
        .get(&DataKey::TotalVaultBalance)
        .unwrap_or_else(|| Vec::new(env));

    transactions.push_back(transaction);

    env.storage()
        .instance()
        .set(&DataKey::TotalVaultBalance, &transactions);
}

/// Retrieve transaction history
pub fn get_transaction_history(env: &Env) -> Vec<TransactionLog> {
    env.storage()
        .instance()
        .get(&DataKey::TotalVaultBalance)
        .unwrap_or_else(|| Vec::new(env))
}
