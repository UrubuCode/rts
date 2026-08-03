//! Which function an address is in.
//!
//! The question a stack walk asks, one return address at a time. Without an
//! answer a stack trace is a list of numbers, and the people who can read those
//! are the people who did not need the trace.

/// An address, said back in the terms a person asked it in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Attribution<'a> {
    /// The function it is in.
    pub function: &'a str,
    /// How far into that function.
    pub offset: u32,
    /// Which part of the client's program that code came from, if anything said.
    pub position: crate::fault::Position,
}

/// One compiled function's place in memory.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CodeRange {
    /// What it is called.
    ///
    /// The name it was declared under. Kept as given: this layer does not
    /// shorten, demangle or prettify, because every one of those is a guess
    /// about a convention it does not own.
    pub name: String,
    /// Where it starts.
    pub start: usize,
    /// How many bytes it occupies.
    pub length: usize,
    /// Where each run of its code came from.
    ///
    /// Carried here rather than kept in a second table beside this one, because
    /// the two are always consulted together: an address finds a function, and
    /// the next thing anyone wants is which part of the program that was. Two
    /// tables would be two lookups and one more thing to keep in agreement.
    pub positions: crate::observe::PositionMap,
}

impl CodeRange {
    /// Whether an address falls inside this function.
    pub fn contains(&self, address: usize) -> bool {
        address >= self.start && address < self.start + self.length
    }

    /// How far into the function an address is.
    pub fn offset_of(&self, address: usize) -> Option<u32> {
        self.contains(address)
            .then(|| (address - self.start) as u32)
    }
}

/// Where every compiled function is.
///
/// Ordered by address and searched by bisection, because a stack walk asks this
/// once per frame and a scan would make walking a deep stack quadratic — which
/// is exactly the case where someone is looking at a trace.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CodeMap {
    ranges: Vec<CodeRange>,
}

impl CodeMap {
    /// A map of nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records where a function ended up.
    ///
    /// Only meaningful once addresses are real, which is after everything is
    /// finalized — before that a function has a length but not a place. Building
    /// this earlier would record a number that is about to change.
    pub fn record(
        &mut self,
        name: impl Into<String>,
        start: usize,
        length: usize,
        positions: crate::observe::PositionMap,
    ) {
        let range = CodeRange {
            name: name.into(),
            start,
            length,
            positions,
        };
        let position = self
            .ranges
            .binary_search_by_key(&start, |existing| existing.start)
            .unwrap_or_else(|insert_at| insert_at);
        self.ranges.insert(position, range);
    }

    /// Which function an address is in, and how far into it.
    pub fn at(&self, address: usize) -> Option<(&CodeRange, u32)> {
        let index = match self
            .ranges
            .binary_search_by_key(&address, |range| range.start)
        {
            Ok(exact) => exact,
            Err(0) => return None,
            Err(after) => after - 1,
        };
        let range = &self.ranges[index];
        range.offset_of(address).map(|offset| (range, offset))
    }

    /// Says an address back in the terms a person asked it in.
    ///
    /// The composed question, and the only one anyone actually has: a return
    /// address off a stack, answered as a function and a place in the program.
    /// Answering the two halves separately and leaving a caller to join them is
    /// how one of the halves ends up consulted with the other's offset.
    pub fn attribute(&self, address: usize) -> Option<Attribution<'_>> {
        let (range, offset) = self.at(address)?;
        Some(Attribution {
            function: &range.name,
            offset,
            position: range.positions.at(offset).unwrap_or_default(),
        })
    }

    /// Every function, in address order.
    pub fn iter(&self) -> impl Iterator<Item = &CodeRange> {
        self.ranges.iter()
    }

    /// How many functions are recorded.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::PositionMap;

    #[test]
    fn an_address_finds_the_function_containing_it() {
        let mut map = CodeMap::new();
        map.record("second", 0x2000, 0x80, PositionMap::default());
        map.record("first", 0x1000, 0x100, PositionMap::default());

        let (range, offset) = map.at(0x1040).expect("inside the first");
        assert_eq!(range.name, "first");
        assert_eq!(offset, 0x40);
    }

    #[test]
    fn an_address_in_the_gap_between_functions_finds_nothing() {
        let mut map = CodeMap::new();
        map.record("first", 0x1000, 0x10, PositionMap::default());
        map.record("second", 0x2000, 0x10, PositionMap::default());

        assert!(
            map.at(0x1500).is_none(),
            "reporting the nearest function would name the wrong one confidently"
        );
        assert!(map.at(0x0500).is_none());
    }

    #[test]
    fn functions_are_kept_in_address_order_however_they_arrive() {
        let mut map = CodeMap::new();
        for start in [0x3000, 0x1000, 0x2000] {
            map.record("f", start, 0x10, PositionMap::default());
        }

        let starts: Vec<_> = map.iter().map(|range| range.start).collect();
        assert_eq!(starts, vec![0x1000, 0x2000, 0x3000]);
    }
}
