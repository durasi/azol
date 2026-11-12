// lru: a least-recently-used cache: a map plus an order list.
// Algorithm: a map from key to slot and a doubly linked list threaded
// through index lists; one `put` for every three `get`s, keys skewed toward
// small numbers.
// Sizes: 3,000,000 operations, capacity 1,024; small: 100,000 operations.
// Output: hits, the sum of found values modulo 1000, entries held.
// This variant replaces the standard SipHash by an FNV-1a hasher written here.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv(0xcbf29ce484222325)
    }
}

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ *byte as u64).wrapping_mul(0x100000001b3);
        }
    }

    fn write_i64(&mut self, n: i64) {
        self.0 = (self.0.rotate_left(5) ^ n as u64).wrapping_mul(0x2545f4914f6cdd1d);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<Fnv>>;

const NONE: usize = usize::MAX;

struct Lru {
    slots: FastMap<String, usize>,
    keys: Vec<String>,
    values: Vec<i64>,
    before: Vec<usize>,
    after: Vec<usize>,
    newest: usize,
    oldest: usize,
    capacity: usize,
}

impl Lru {
    fn unlink(&mut self, i: usize) {
        let b = self.before[i];
        let a = self.after[i];
        if b != NONE {
            self.after[b] = a;
        } else {
            self.newest = a;
        }
        if a != NONE {
            self.before[a] = b;
        } else {
            self.oldest = b;
        }
    }

    fn link_newest(&mut self, i: usize) {
        self.before[i] = NONE;
        self.after[i] = self.newest;
        if self.newest != NONE {
            self.before[self.newest] = i;
        } else {
            self.oldest = i;
        }
        self.newest = i;
    }

    fn get(&mut self, key: &str) -> Option<i64> {
        let i = *self.slots.get(key)?;
        self.unlink(i);
        self.link_newest(i);
        Some(self.values[i])
    }

    fn put(&mut self, key: String, value: i64) {
        if let Some(&i) = self.slots.get(&key) {
            self.values[i] = value;
            self.unlink(i);
            self.link_newest(i);
            return;
        }
        let mut i = self.keys.len();
        if i < self.capacity {
            self.keys.push(key.clone());
            self.values.push(value);
            self.before.push(NONE);
            self.after.push(NONE);
        } else {
            i = self.oldest;
            self.slots.remove(&self.keys[i]);
            self.unlink(i);
            self.keys[i] = key.clone();
            self.values[i] = value;
        }
        self.slots.insert(key, i);
        self.link_newest(i);
    }
}

fn main() {
    let count = size(3000000, 100000);
    let mut c = Lru {
        slots: FastMap::default(),
        keys: Vec::new(),
        values: Vec::new(),
        before: Vec::new(),
        after: Vec::new(),
        newest: NONE,
        oldest: NONE,
        capacity: 1024,
    };
    let mut seed: i64 = 29;
    let mut hits = 0;
    let mut total = 0;
    for n in 0..count {
        seed = seed * 48271 % 2147483647;
        let key = format!("k{}", seed % 64 * (seed / 64 % 64));
        if n % 4 == 0 {
            c.put(key, n);
        } else if let Some(found) = c.get(&key) {
            hits += 1;
            total += found % 1000;
        }
    }
    println!("{} {} {}", hits, total, c.keys.len());
}
