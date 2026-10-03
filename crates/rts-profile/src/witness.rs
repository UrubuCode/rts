//! What arrived at a site, in terms that survive a recompilation.

use std::fmt;

/// One thing a site was observed to receive, named rather than numbered.
///
/// # Why a list of names and not a kind per observation
///
/// Because one shape covers every subject worth recording and none of them is a
/// language concept:
///
/// | the site asks | the witness is |
/// |---|---|
/// | which member was reached | one name |
/// | which function was called | one name — the callee's stable symbol |
/// | what layout the receiver had | the names it holds, in order |
///
/// A variant per subject would have had to name those subjects, and "member",
/// "callee" and "layout" are a language's vocabulary for its own sites. The
/// witness does not need to know which question it answers: a `Position`
/// identifies exactly one site, and whoever minted that position knows what it
/// asked.
///
/// # Why names and not the numbers the compiler has in hand
///
/// A key number, a shape id and a cache index are all minted in the order one
/// compilation happens to ask for them, so a record carrying them is valid only
/// for that compilation — and reads back pointing at other sites rather than
/// failing. Names cost bytes and are the only form that crosses two
/// compilations. `docs/engine/profile-oracle.md` has the three candidates and
/// why each fails.
///
/// # Order is part of the identity
///
/// `{x, y}` and `{y, x}` are two witnesses, because for a layout arrived at one
/// member at a time they are two layouts — `rts_cranelift::shape` builds a
/// distinct node per addition. Sorting the names to make them compare equal
/// would merge two populations into one and report a majority that no site ever
/// saw.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Witness(Box<[Box<str>]>);

impl Witness {
    /// A witness of one name — a member reached, a function called.
    pub fn of(name: &str) -> Self {
        Witness(Box::new([name.into()]))
    }

    /// A witness of several names, in the order the site saw them.
    pub fn of_all<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Witness(names.into_iter().map(|name| name.as_ref().into()).collect())
    }

    /// The names, in the order they were recorded.
    pub fn names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.iter().map(|name| &**name)
    }

    /// How many names this witness carries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this witness carries no names.
    ///
    /// Not a defect: a layout with no members is a real observation, and a site
    /// that keeps receiving one is saying something worth acting on.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Witness {
    /// The names separated by a space, which is what the record's text form
    /// writes. A name may not contain whitespace; [`crate::Profile`] is where
    /// that is refused, because it is a property of the FILE rather than of the
    /// witness.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, name) in self.0.iter().enumerate() {
            if at > 0 {
                formatter.write_str(" ")?;
            }
            formatter.write_str(name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two orders of the same names are two witnesses.
    ///
    /// Merging them would report a majority no site saw, which is the failure
    /// this is a test for rather than an assertion about `Ord`.
    #[test]
    fn order_distinguishes_two_layouts() {
        assert_ne!(Witness::of_all(["x", "y"]), Witness::of_all(["y", "x"]));
    }

    /// A witness reads back the names it was given, unchanged and in order.
    #[test]
    fn the_names_survive_being_recorded() {
        let witness = Witness::of_all(["first", "second", "third"]);
        assert_eq!(
            witness.names().collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_eq!(witness.len(), 3);
        assert_eq!(witness.to_string(), "first second third");
    }

    /// A site that sees nothing in particular still produces a witness.
    #[test]
    fn an_empty_witness_is_an_observation() {
        let empty = Witness::of_all(Vec::<&str>::new());
        assert!(empty.is_empty());
        assert_eq!(empty.to_string(), "");
    }
}
