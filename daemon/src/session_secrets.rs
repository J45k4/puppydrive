use std::collections::HashMap;

use secrecy::SecretString;

#[derive(Default)]
pub struct SessionSecretStore {
    secrets: HashMap<(String, String), SecretString>,
}

#[allow(dead_code)] // Used by remote source providers once those transports are enabled.
impl SessionSecretStore {
    pub fn set(&mut self, source_key: &str, slot: &str, value: SecretString) {
        self.secrets
            .insert((source_key.to_owned(), slot.to_owned()), value);
    }

    pub fn is_available(&self, source_key: &str, slot: &str) -> bool {
        self.secrets
            .contains_key(&(source_key.to_owned(), slot.to_owned()))
    }

    pub fn clear_source(&mut self, source_key: &str) {
        self.secrets
            .retain(|(stored_source, _), _| stored_source != source_key);
    }

    pub fn clear(&mut self) {
        self.secrets.clear();
    }
}

impl Drop for SessionSecretStore {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_scoped_to_source_and_slot() {
        let mut store = SessionSecretStore::default();
        store.set("source-a", "password", SecretString::from("secret"));
        assert!(store.is_available("source-a", "password"));
        assert!(!store.is_available("source-b", "password"));
        store.clear_source("source-a");
        assert!(!store.is_available("source-a", "password"));
    }
}
