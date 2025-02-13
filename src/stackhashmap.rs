use std::collections::HashMap;

#[derive(Debug)]
struct StackHashMap<K, V> {
    data: Vec<HashMap<K, V>>,
}

impl<K: Eq + std::hash::Hash, V> StackHashMap<K, V> {
    fn new() -> Self {
        StackHashMap {
            // TODO: is non empty data an invarient? if not, should this start empty?
            data: vec![HashMap::new()],
        }
    }

    fn height(&self) -> usize {
        self.data.len()
    }

    fn push_frame(&mut self) {
        self.data.push(HashMap::new())
    }

    fn pop_frame(&mut self) {
        self.data.pop();
        // TODO: do we want this invarient?
        assert!(!self.data.is_empty());
    }

    fn get(&self, k: &K) -> Option<&V> {
        self.data.iter().rev().flat_map(|hm| hm.get(k)).next()
    }

    fn insert(&mut self, k: K, v: V) -> Option<V> {
        // TODO: if not being empty is an invarient we could unwrap / expect here
        self.data.last_mut().and_then(|hm| hm.insert(k, v))
    }

    fn scope<F: FnOnce(&mut Self)>(&mut self, f: F) {
        let prev_height = self.height();
        self.push_frame();
        f(self);
        self.pop_frame();
        assert_eq!(self.height(), prev_height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_works() {
        let shm: StackHashMap<String, u8> = StackHashMap::new();
        dbg!(shm);
    }

    #[test]
    fn get_reads() {
        let mut shm = StackHashMap::new();
        shm.insert("k", 5);
        shm.insert("v", 6);
        assert_eq!(*shm.get(&"k").unwrap(), 5);
    }
    #[test]
    fn get_reads_any_remaining_layer() {
        let mut shm = StackHashMap::new();

        shm.insert("k", 5);

        shm.push_frame();
        shm.insert("v", 6);

        assert_eq!(*shm.get(&"k").unwrap(), 5);
        assert_eq!(*shm.get(&"v").unwrap(), 6);

        shm.pop_frame();
        assert_eq!(*shm.get(&"k").unwrap(), 5);
        assert_eq!(shm.get(&"v"), None);
    }

    #[test]
    fn shadowing() {
        let mut shm = StackHashMap::new();

        shm.insert("k", 5);

        shm.push_frame();
        shm.insert("k", 6);

        assert_eq!(*shm.get(&"k").unwrap(), 6);

        shm.pop_frame();
        assert_eq!(*shm.get(&"k").unwrap(), 5);
    }

    #[test]
    fn scope() {
        let mut shm = StackHashMap::new();

        shm.insert(0, true);
        shm.insert(1, true);
        assert_eq!(*shm.get(&1).unwrap(), true);
        assert_eq!(shm.height(), 1);

        shm.scope(|shm| {
            assert_eq!(shm.height(), 2);
            shm.insert(1, false);
            assert_eq!(*shm.get(&1).unwrap(), false);
        });

        assert_eq!(*shm.get(&1).unwrap(), true);
        assert_eq!(shm.height(), 1);
        shm.scope(|shm| {
            shm.scope(|shm| {
                shm.scope(|shm| {
                    assert_eq!(shm.height(), 4);
                    assert_eq!(*shm.get(&1).unwrap(), true);
                })
            })
        })
    }
}
