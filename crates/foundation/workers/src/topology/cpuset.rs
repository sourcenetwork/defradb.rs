//! A set of logical CPU ids, written and read in the kernel's list format
//! (`0-3,8,10-11`).

use std::fmt;

use crate::Error;

/// The highest CPU id a set holds, plus one. Linux builds top out at 8192
/// CPUs; this leaves room above that while bounding what a malformed list
/// (`0-4294967295`) could make a set allocate.
pub const MAX_CPUS: usize = 1 << 16;

const BITS: usize = u64::BITS as usize;

/// A set of logical CPU ids.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct CpuSet {
    words: Vec<u64>,
}

impl CpuSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses the kernel's list format: comma-separated ids and inclusive
    /// ranges, surrounding whitespace ignored. An empty string is an empty set.
    pub fn parse(list: &str) -> Result<Self, Error> {
        let mut set = Self::new();
        for part in list
            .trim()
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let (low, high) = match part.split_once('-') {
                Some((low, high)) => (parse_id(low, list)?, parse_id(high, list)?),
                None => {
                    let id = parse_id(part, list)?;
                    (id, id)
                }
            };
            if low > high {
                return Err(Error::Config(format!(
                    "CPU range {part} in {list:?} runs backwards"
                )));
            }
            (low..=high).for_each(|cpu| set.insert(cpu));
        }
        Ok(set)
    }

    /// Adds `cpu`. Ids at or above [`MAX_CPUS`] are ignored.
    pub fn insert(&mut self, cpu: usize) {
        if cpu >= MAX_CPUS {
            return;
        }
        let word = cpu / BITS;
        if self.words.len() <= word {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1 << (cpu % BITS);
    }

    /// Whether `cpu` is in the set.
    pub fn contains(&self, cpu: usize) -> bool {
        self.words
            .get(cpu / BITS)
            .is_some_and(|word| word & (1 << (cpu % BITS)) != 0)
    }

    /// How many CPUs the set holds.
    pub fn len(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Whether the set holds no CPU.
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|w| *w == 0)
    }

    /// The lowest id in the set.
    pub fn first(&self) -> Option<usize> {
        self.iter().next()
    }

    /// The ids, in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(index, word)| {
            let mut rest = *word;
            std::iter::from_fn(move || {
                if rest == 0 {
                    return None;
                }
                let bit = rest.trailing_zeros() as usize;
                rest &= rest - 1;
                Some(index * BITS + bit)
            })
        })
    }

    /// The CPUs in both sets.
    pub fn intersection(&self, other: &CpuSet) -> CpuSet {
        CpuSet {
            words: self
                .words
                .iter()
                .zip(&other.words)
                .map(|(a, b)| a & b)
                .collect(),
        }
    }

    /// Adds every CPU of `other`.
    pub fn union_with(&mut self, other: &CpuSet) {
        if self.words.len() < other.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        self.words
            .iter_mut()
            .zip(&other.words)
            .for_each(|(a, b)| *a |= b);
    }

    /// Removes every CPU of `other`.
    pub fn remove_all(&mut self, other: &CpuSet) {
        self.words
            .iter_mut()
            .zip(&other.words)
            .for_each(|(a, b)| *a &= !b);
    }
}

fn parse_id(text: &str, list: &str) -> Result<usize, Error> {
    let id: usize = text
        .trim()
        .parse()
        .map_err(|_| Error::Config(format!("{text:?} in CPU list {list:?} is not a CPU id")))?;
    if id >= MAX_CPUS {
        return Err(Error::Config(format!(
            "CPU {id} in {list:?} is above the supported {MAX_CPUS}"
        )));
    }
    Ok(id)
}

impl FromIterator<usize> for CpuSet {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = usize>,
    {
        let mut set = CpuSet::new();
        iter.into_iter().for_each(|cpu| set.insert(cpu));
        set
    }
}

impl fmt::Display for CpuSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ids = self.iter().peekable();
        let mut first = true;
        while let Some(low) = ids.next() {
            let mut high = low;
            while ids.peek() == Some(&(high + 1)) {
                high += 1;
                ids.next();
            }
            if !first {
                f.write_str(",")?;
            }
            first = false;
            if low == high {
                write!(f, "{low}")?;
            } else {
                write!(f, "{low}-{high}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for CpuSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CpuSet({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_and_ranges_and_prints_them_back() {
        let set = CpuSet::parse(" 0-3,8, 10-11\n").unwrap();
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(set.to_string(), "0-3,8,10-11");
        assert_eq!(set.len(), 7);
        assert_eq!(set.first(), Some(0));
    }

    #[test]
    fn an_empty_list_is_an_empty_set() {
        let set = CpuSet::parse("\n").unwrap();
        assert!(set.is_empty());
        assert_eq!(set.to_string(), "");
    }

    #[test]
    fn rejects_garbage_backwards_ranges_and_huge_ids() {
        assert!(CpuSet::parse("a").is_err());
        assert!(CpuSet::parse("3-1").is_err());
        assert!(CpuSet::parse("0-4294967295").is_err());
    }

    #[test]
    fn set_operations_cross_word_boundaries() {
        let a: CpuSet = [1, 63, 64, 130].into_iter().collect();
        let b: CpuSet = [63, 130, 200].into_iter().collect();
        assert_eq!(a.intersection(&b).to_string(), "63,130");
        let mut u = a.clone();
        u.union_with(&b);
        assert_eq!(u.to_string(), "1,63-64,130,200");
        u.remove_all(&a);
        assert_eq!(u.to_string(), "200");
        assert!(!u.contains(64));
    }
}
