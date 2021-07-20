use std::collections::HashMap;
use unicase::UniCase;

#[derive(Debug, Default, Clone)]
pub struct CaseInsensitiveHashMap<T> {
    map: HashMap<UniCase<String>, T>,
}

impl<T> CaseInsensitiveHashMap<T> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        let uni_case = UniCase::new(key.to_string());
        self.map.get(&uni_case)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut T> {
        let uni_case = UniCase::new(key.to_string());
        self.map.get_mut(&uni_case)
    }

    pub fn insert(&mut self, key: &str, val: T) -> Option<T> {
        let uni_case = UniCase::new(key.to_string());
        self.map.insert(uni_case, val)
    }
}
