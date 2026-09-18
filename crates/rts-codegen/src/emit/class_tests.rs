//! What the class lowering relies on from the name table, pinned apart from it.

use crate::names::Names;

#[test]
fn a_private_key_and_its_public_namesake_are_still_different_identifiers() {
    // Reusing the `Name` `Cx::private_name` already produced means the
    // separation this module relies on has to come from THAT interning, not
    // from anything here — this test pins that it still does.
    let mut names = Names::new();
    let private = names.intern("@@#x");
    let public = names.intern("x");
    assert_ne!(private, public);
}
