use std::collections::HashMap;

pub struct AutoIdMap<T> {
    last_id: usize,
    map: HashMap<usize, T>,
}

impl<T> AutoIdMap<T> {
    pub fn new() -> AutoIdMap<T> {
        AutoIdMap {
            last_id: 0,
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, elem: T) -> usize {
        self.last_id += 1;
        self.map.insert(self.last_id, elem);
        self.last_id
    }

    pub fn replace(&mut self, id: &usize, elem: T) {
        // because we really don't want you to abuse this to insert your own id's :)
        if !self.contains_key(id) {
            panic!("no entry to replace for {}", id);
        }
        self.map.insert(id.clone(), elem);
    }

    pub fn get(&self, id: &usize) -> Option<&T> {
        self.map.get(id)
    }

    pub fn remove(&mut self, id: &usize) -> T {
        self.map.remove(id).expect("no such elem")
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn contains_key(&self, id: &usize) -> bool {
        self.map.contains_key(id)
    }
}

#[cfg(test)]
pub mod tests {
    use crate::utils::AutoIdMap;

    #[test]
    fn test_auto_id_map() {
        let mut map = AutoIdMap::new();
        let id1 = map.insert("hi");
        let id2 = map.insert("hi2");
        assert_ne!(id1, id2);
        assert_eq!(map.len(), 2);
        let s1 = map.remove(&id1);
        assert_eq!(s1, "hi");
        assert_eq!(map.len(), 1);
    }
}
