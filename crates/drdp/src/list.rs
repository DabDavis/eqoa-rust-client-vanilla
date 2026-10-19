//! The two intrusive containers, as ordered vectors. A `vlist` with a
//! comparator inserts after equal keys (`vlist_insert`); a `vtree` puts an
//! equal key to the RIGHT (`vtree_insert_raw`'s `cmp >= 0`), so its in-order
//! walk is the same "sorted, insert after equals" order. Both are kept as a
//! `Vec` of handles sorted by the original comparator.

use std::cmp::Ordering;

/// `uns32_cmp_wrap`: by the sign of the wrapped difference.
pub fn cmp_u32_wrap(a: u32, b: u32) -> Ordering {
    (a.wrapping_sub(b) as i32).cmp(&0)
}

/// `uns16_cmp_wrap`.
pub fn cmp_u16_wrap(a: u16, b: u16) -> Ordering {
    (a.wrapping_sub(b) as i16).cmp(&0)
}

/// `vlist_insert` / `vtree_insert` order: before the first element whose key
/// is greater, scanning as `vlist_insert` does (head, tail, then from the
/// second element).
pub fn insert_sorted<T: Copy>(v: &mut Vec<T>, item: T, key: impl Fn(T) -> u64, cmp: impl Fn(u64, u64) -> Ordering) {
    let k = key(item);
    if v.is_empty() || cmp(k, key(v[0])) == Ordering::Less {
        v.insert(0, item);
        return;
    }
    if cmp(key(*v.last().unwrap()), k) != Ordering::Greater {
        v.push(item);
        return;
    }
    let mut i = 1;
    while i < v.len() && cmp(k, key(v[i])) != Ordering::Less {
        i += 1;
    }
    v.insert(i, item);
}

/// `vlist_lookup`: the first element equal to `k`, scanning from the head and
/// giving up at the first greater one.
pub fn lookup_sorted<T: Copy>(v: &[T], k: u64, key: impl Fn(T) -> u64, cmp: impl Fn(u64, u64) -> Ordering) -> Option<T> {
    for &x in v {
        match cmp(k, key(x)) {
            Ordering::Less => return None,
            Ordering::Equal => return Some(x),
            Ordering::Greater => {}
        }
    }
    None
}

/// Removes the first occurrence of `item`.
pub fn remove<T: PartialEq>(v: &mut Vec<T>, item: &T) -> bool {
    match v.iter().position(|x| x == item) {
        Some(i) => {
            v.remove(i);
            true
        }
        None => false,
    }
}

/// A simple arena: handles stay valid until removed.
#[derive(Debug)]
pub struct Arena<T> {
    items: Vec<Option<T>>,
    free: Vec<usize>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena { items: Vec::new(), free: Vec::new() }
    }
}

impl<T> Arena<T> {
    pub fn insert(&mut self, t: T) -> usize {
        match self.free.pop() {
            Some(i) => {
                self.items[i] = Some(t);
                i
            }
            None => {
                self.items.push(Some(t));
                self.items.len() - 1
            }
        }
    }
    pub fn remove(&mut self, i: usize) -> Option<T> {
        let t = self.items.get_mut(i)?.take();
        if t.is_some() {
            self.free.push(i);
        }
        t
    }
    pub fn get(&self, i: usize) -> Option<&T> {
        self.items.get(i)?.as_ref()
    }
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.items.get_mut(i)?.as_mut()
    }
    pub fn contains(&self, i: usize) -> bool {
        self.get(i).is_some()
    }
}

impl<T> std::ops::Index<usize> for Arena<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.get(i).expect("dangling handle")
    }
}

impl<T> std::ops::IndexMut<usize> for Arena<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        self.get_mut(i).expect("dangling handle")
    }
}
