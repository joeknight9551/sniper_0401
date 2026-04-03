use crate::*;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub static TOKEN_DB: Lazy<TokenDatabase> = Lazy::new(|| TokenDatabase {
    map: Arc::new(DashMap::new()),
});

#[derive(Clone, Debug)]
pub struct TokenDatabase {
    pub map: Arc<DashMap<Pubkey, TokenDatabaseSchema>>,
}

impl TokenDatabase {
    pub fn upsert(&self, key: Pubkey, data: TokenDatabaseSchema) -> Result<(), BoxError> {
        self.map.insert(key, data.clone());
        Ok(())
    }

    pub fn get(&self, key: Pubkey) -> Result<Option<TokenDatabaseSchema>, BoxError> {
        Ok(self.map.get(&key).map(|data| data.clone()))
    }

    pub fn get_list_all(&self) -> Result<Vec<(Pubkey, TokenDatabaseSchema)>, BoxError> {
        let mut results = Vec::new();
        for r in self.map.iter() {
            results.push((r.key().clone(), r.value().clone()));
        }
        Ok(results)
    }

    pub fn delete(&self, key: Pubkey) -> Result<(), BoxError> {
        self.map.remove(&key);
        Ok(())
    }
}
