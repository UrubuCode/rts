//! A second client, because a boundary with one client is not a boundary.
//!
//! `docs/engine/a-second-language.md`: *"a boundary with one client on each side
//! is indistinguishable from no boundary at all"*. `rts-mir` answers that with
//! `tests/toy_domain.rs` — three types, integer distinct from float, a two-case
//! truth rule — and this file is the same device for the record: a language that
//! is not JavaScript, writing and reading profiles with no front end present.
//!
//! What it is checking is not that the code runs. It is that **every decision
//! which is language meaning happens in this file rather than in the crate**:
//! the threshold, what a majority authorises, what a name means, and whether an
//! overflowed site disqualifies itself. If any of those could be moved into
//! `rts-profile` and still be right, the crate has acquired a language.

use rts_cranelift::fault::Position;
use rts_profile::{ModuleId, Profile, SiteKey, WITNESS_WIDTH, Witness, read, write};

/// The toy language's three types. Integer is distinct from float, which is the
/// divergence `rts_mir::domain` names as the reason a shared lattice is worse
/// than a parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ToyType {
    Integer,
    Float,
    Table,
}

/// What this language is willing to bet on, and the share it demands.
///
/// **This is the part that may not move into the crate.** The number and the
/// rule are both judgements about one language: a language whose fall is cheap
/// can speculate at 80%, one whose generic body is nearly as fast as its
/// specialised one should not speculate at all, and one with no distinction
/// between integer and float has no use for the first arm below.
fn what_the_toy_language_assumes(profile: &Profile, site: SiteKey) -> Option<ToyType> {
    let majority = profile.at(site)?.majority()?;
    if majority.overflowed {
        return None;
    }
    if majority.count * 100 < majority.total * 90 {
        return None;
    }
    match majority.witness.names().next()? {
        "int" if majority.witness.len() == 1 => Some(ToyType::Integer),
        "flt" if majority.witness.len() == 1 => Some(ToyType::Float),
        _ => Some(ToyType::Table),
    }
}

fn module() -> ModuleId {
    ModuleId::of_source("local t = {}\nt.n = 1\nreturn t\n")
}

/// A language with no JavaScript in it records, writes, reads and decides.
///
/// The whole loop, because each half is useless alone: a record that cannot be
/// written is one run's data, and one that cannot be read back is a file.
#[test]
fn a_second_language_completes_the_loop() {
    let integer_site = SiteKey::new(module(), Position(11));
    let float_site = SiteKey::new(module(), Position(22));
    let table_site = SiteKey::new(module(), Position(33));

    let mut profile = Profile::new();
    for _ in 0..1000 {
        profile.saw(integer_site, &Witness::of("int"));
    }
    for _ in 0..1000 {
        profile.saw(float_site, &Witness::of("flt"));
    }
    for _ in 0..1000 {
        profile.saw(table_site, &Witness::of_all(["n"]));
    }

    let after = read(&write(&profile).expect("names without whitespace")).expect("own record");

    assert_eq!(
        what_the_toy_language_assumes(&after, integer_site),
        Some(ToyType::Integer),
    );
    assert_eq!(
        what_the_toy_language_assumes(&after, float_site),
        Some(ToyType::Float),
        "integer and float came back indistinguishable, which is the one \
         divergence this domain exists to have"
    );
    assert_eq!(
        what_the_toy_language_assumes(&after, table_site),
        Some(ToyType::Table),
    );
}

/// A site below this language's threshold authorises nothing — and the crate
/// still answers the counts.
///
/// The distinction being pinned: declining is the language's act. The record is
/// identical in both directions and says so in numbers, which is what lets two
/// languages read one profile and disagree about it.
#[test]
fn the_threshold_is_the_languages_and_not_the_records() {
    let site = SiteKey::new(module(), Position(44));
    let mut profile = Profile::new();
    for _ in 0..70 {
        profile.saw(site, &Witness::of("int"));
    }
    for _ in 0..30 {
        profile.saw(site, &Witness::of("flt"));
    }

    assert_eq!(what_the_toy_language_assumes(&profile, site), None);

    let majority = profile.at(site).and_then(|o| o.majority()).expect("seen");
    assert_eq!(majority.count, 70);
    assert_eq!(majority.total, 100);
    assert!(
        !majority.overflowed,
        "a site with two witnesses was reported as overflowed"
    );
}

/// A megamorphic site is reported as one, through a record and back.
#[test]
fn a_megamorphic_site_survives_as_megamorphic() {
    let site = SiteKey::new(module(), Position(55));
    let mut profile = Profile::new();
    for at in 0..WITNESS_WIDTH + 50 {
        profile.saw(site, &Witness::of(&format!("t{at}")));
    }

    let after = read(&write(&profile).expect("representable")).expect("own record");
    assert!(after.at(site).expect("the site").overflowed());
    assert_eq!(what_the_toy_language_assumes(&after, site), None);
}

/// Editing the toy module discards its record rather than reinterpreting it.
///
/// The failure this is a test for is silent: the positions of the old text read
/// against the new, every guard still passing, the speculation pointed at
/// whatever is now at that offset.
#[test]
fn editing_the_module_discards_its_record() {
    let mut profile = Profile::new();
    profile.saw(SiteKey::new(module(), Position(11)), &Witness::of("int"));

    let edited = ModuleId::of_source("local t = {}\nt.n = 2\nreturn t\n");
    assert!(profile.knows(module()));
    assert!(!profile.knows(edited));
    assert_eq!(
        what_the_toy_language_assumes(&profile, SiteKey::new(edited, Position(11))),
        None,
        "an edited module read the old module's observations at the same offset"
    );
}
