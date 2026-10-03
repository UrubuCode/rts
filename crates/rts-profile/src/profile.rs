//! Every site's record, and the refusal that keeps a stale one out.

use std::collections::BTreeMap;

use rts_cranelift::fault::Position;

use crate::module::ModuleId;
use crate::observation::{Observation, Recorder};
use crate::witness::Witness;

/// Which site an observation belongs to.
///
/// # Why the position and not an index the compiler minted
///
/// `rts_cranelift::fault::Position` is already defined as *"a number the client
/// gave us"* that the machine never interprets, never orders and never renders
/// — and the client is the only party that can make it stable across two
/// compilations. Every alternative is minted in the order one compilation asks:
/// a `CacheId` is a dense table index, a `Key` comes from a registry that
/// issues them in request order, a shape id is created as the program grows.
///
/// Reusing the position is also what stops this crate from being a second
/// numbering of one space, which `reuse-check` section 3 calls a bug rather
/// than redundancy.
///
/// # One position is one site
///
/// There is deliberately no field saying *what kind* of site this is. A position
/// identifies exactly one place in the client's program, so whoever minted it
/// knows what it asked there; a kind field would have to enumerate a language's
/// own vocabulary for its sites, in a crate that may not name one. The
/// consequence is a rule for the writer: two distinct observable things at one
/// source place need two positions.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SiteKey {
    /// Which module, by the identity its source decides.
    pub module: ModuleId,
    /// Where in that module, in the client's own numbering.
    pub position: Position,
}

impl SiteKey {
    /// A key for this position in this module.
    pub fn new(module: ModuleId, position: Position) -> Self {
        SiteKey { module, position }
    }
}

/// What a run of a program observed, keyed by site.
///
/// Ordered rather than hashed, because a record is written to a file a person
/// diffs between builds — `rts-cranelift` rule 13. A map whose iteration order
/// came from a hash would produce a different file from the same observations.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Profile {
    sites: BTreeMap<SiteKey, Observation>,
}

impl Profile {
    /// A profile that observed nothing.
    pub fn new() -> Self {
        Profile::default()
    }

    /// Records one arrival at a site.
    pub fn saw(&mut self, site: SiteKey, witness: &Witness) {
        self.sites.entry(site).or_default().saw(witness);
    }

    /// Files a finished recorder under its site.
    ///
    /// What the writing side does when a run ends: the hot path holds a
    /// [`Recorder`] per site and hands it over once, rather than reaching into
    /// a map on every miss.
    pub fn file(&mut self, site: SiteKey, recorder: Recorder) {
        let observation = recorder.finish();
        self.sites
            .entry(site)
            .or_default()
            .absorb(&observation);
    }

    /// What a site was observed to receive.
    pub fn at(&self, site: SiteKey) -> Option<&Observation> {
        self.sites.get(&site)
    }

    /// Whether this profile says anything about a module.
    ///
    /// **This is the refusal, and it is the whole safety argument.** A reader
    /// computes the identity from the source it is about to compile and asks
    /// here. An edited module answers a different identity, so it has no record,
    /// so nothing is speculated from observations taken against text that no
    /// longer exists — rather than the positions being reinterpreted against
    /// whatever is now at that offset, which is the silent failure
    /// `docs/engine/profile-oracle.md` is organised around.
    pub fn knows(&self, module: ModuleId) -> bool {
        self.sites
            .range(
                SiteKey::new(module, Position(0))
                    ..=SiteKey::new(module, Position(u32::MAX)),
            )
            .next()
            .is_some()
    }

    /// Every site, in a deterministic order.
    pub fn sites(&self) -> impl ExactSizeIterator<Item = (&SiteKey, &Observation)> {
        self.sites.iter()
    }

    /// How many sites this profile holds.
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// Whether this profile holds no site at all.
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// States a site's totals, as reading a record back does.
    ///
    /// Paired with [`Profile::restore`] and used only by [`crate::text`]. Public
    /// because a second reader of the format — another tool, another language's
    /// writer — needs the same two calls, and a format only one module can
    /// reconstruct is a format with one implementation.
    pub fn declare(&mut self, site: SiteKey, total: u64, overflowed: bool) {
        self.sites.entry(site).or_default().declare(total, overflowed);
    }

    /// Puts back one witness of a site, without touching its total.
    pub fn restore(&mut self, site: SiteKey, witness: &Witness, count: u64) {
        self.sites.entry(site).or_default().restore(witness, count);
    }

    /// Adds another profile's observations into this one.
    ///
    /// Two runs of the same program, or two processes of one corpus. A site both
    /// profiles saw has its counts added; overflow is sticky, because a site
    /// that was megamorphic in either run was megamorphic.
    pub fn absorb(&mut self, other: &Profile) {
        for (site, observation) in other.sites() {
            self.sites.entry(*site).or_default().absorb(observation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module() -> ModuleId {
        ModuleId::of_source("let point = { x: 1, y: 2 }\n")
    }

    /// A profile taken against one module says nothing about an edited one.
    ///
    /// The test that matters most in this crate. Without it the positions of the
    /// old text are read against the new, every guard still passes, and the
    /// speculation points somewhere else with nothing to report it.
    #[test]
    fn an_edited_module_has_no_record() {
        let mut profile = Profile::new();
        profile.saw(
            SiteKey::new(module(), Position(42)),
            &Witness::of_all(["x", "y"]),
        );

        assert!(profile.knows(module()));
        assert!(
            !profile.knows(ModuleId::of_source("let point = { x: 1, y: 3 }\n")),
            "an edited module was accepted as the one the profile was taken \
             against, so its observations would be attributed to whatever is at \
             those offsets now"
        );
    }

    /// Two sites in one module do not collide, and a position in one module is
    /// not a position in another.
    #[test]
    fn a_site_is_a_module_and_a_position_together() {
        let other = ModuleId::of_source("print(1)\n");
        let mut profile = Profile::new();
        profile.saw(SiteKey::new(module(), Position(1)), &Witness::of("a"));
        profile.saw(SiteKey::new(module(), Position(2)), &Witness::of("b"));
        profile.saw(SiteKey::new(other, Position(1)), &Witness::of("c"));

        assert_eq!(profile.len(), 3);
        assert_eq!(
            profile
                .at(SiteKey::new(module(), Position(1)))
                .and_then(|o| o.majority())
                .map(|m| m.witness.to_string()),
            Some("a".to_owned())
        );
        assert_eq!(
            profile
                .at(SiteKey::new(other, Position(1)))
                .and_then(|o| o.majority())
                .map(|m| m.witness.to_string()),
            Some("c".to_owned())
        );
    }

    /// `knows` answers for the module asked about and not for its neighbours.
    ///
    /// The range query is where an off-by-one would be invisible: a profile that
    /// answered `true` for every module would hand every reader somebody else's
    /// observations.
    #[test]
    fn knowing_one_module_is_not_knowing_all_of_them() {
        let mut profile = Profile::new();
        profile.saw(SiteKey::new(module(), Position(7)), &Witness::of("a"));
        assert!(!profile.knows(ModuleId::from_bits(module().bits() - 1)));
        assert!(!profile.knows(ModuleId::from_bits(module().bits() + 1)));
    }

    /// Two runs of one program add up per site.
    #[test]
    fn two_runs_add_up() {
        let site = SiteKey::new(module(), Position(9));
        let witness = Witness::of_all(["x", "y"]);

        let mut first = Profile::new();
        for _ in 0..3 {
            first.saw(site, &witness);
        }
        let mut second = Profile::new();
        for _ in 0..4 {
            second.saw(site, &witness);
        }
        first.absorb(&second);

        assert_eq!(first.at(site).expect("the site").total(), 7);
    }

    /// A recorder filed at a site lands as that site's observation.
    #[test]
    fn a_filed_recorder_becomes_the_sites_record() {
        let site = SiteKey::new(module(), Position(11));
        let witness = Witness::of("member");
        let mut recorder = Recorder::new();
        recorder.saw(&witness);
        recorder.saw(&witness);

        let mut profile = Profile::new();
        profile.file(site, recorder);

        let majority = profile.at(site).and_then(|o| o.majority()).expect("filed");
        assert_eq!(majority.count, 2);
    }
}
