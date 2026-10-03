//! The form a record takes between two runs, and what it refuses.

use std::fmt;

use rts_cranelift::fault::Position;

use crate::module::ModuleId;
use crate::profile::{Profile, SiteKey};
use crate::witness::Witness;

/// Which spelling of the record this crate writes and accepts.
///
/// A record carrying any other number is refused rather than interpreted. The
/// alternative — reading an older layout as best it can — is the silent class
/// this crate exists to avoid one level up: a field that moved would be read as
/// the field now at that place, every guard would still pass, and the
/// speculation would be pointed somewhere else.
pub const FORMAT_VERSION: u32 = 1;

/// Why a record could not be written, or could not be read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FormatError {
    /// A name cannot be written because the record separates names by spaces.
    ///
    /// Refused at the point of writing rather than escaped. A record is diffed
    /// by people and read by a second implementation some day, and an escaping
    /// rule is a second thing to agree about; a client whose names can contain
    /// whitespace needs a decision here, not a quoting convention invented
    /// under it.
    UnrepresentableName {
        /// The name as it was given.
        name: String,
    },
    /// The record does not begin with this crate's header.
    NotAProfile,
    /// The record is a version this crate does not read.
    Version {
        /// What the record claimed to be.
        found: u32,
    },
    /// A line could not be read as what its position in the record requires.
    Malformed {
        /// Which line, counting from one, as a person reading the file counts.
        line: usize,
        /// What was expected there.
        expected: &'static str,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::UnrepresentableName { name } => write!(
                formatter,
                "the name {name:?} contains whitespace, which the record cannot \
                 separate from the next name"
            ),
            FormatError::NotAProfile => {
                formatter.write_str("not a profile: the first line is not this crate's header")
            }
            FormatError::Version { found } => write!(
                formatter,
                "the record is version {found} and this is version {FORMAT_VERSION}"
            ),
            FormatError::Malformed { line, expected } => {
                write!(formatter, "line {line}: expected {expected}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// The header every record begins with.
const HEADER: &str = "rts-profile";

/// A record of this profile, as text.
///
/// Deterministic: sites in key order, witnesses by descending count with ties
/// broken by the witness itself. Two runs that observed the same thing produce
/// byte-identical records, which is what makes a diff between two builds worth
/// reading — `rts-cranelift` rule 13.
///
/// # Why text
///
/// Because the first thing anyone does with a profile is disbelieve it. A record
/// a person can read tells them which site they are looking at and what it saw;
/// a binary one needs a tool before it can be doubted, and the tool is written
/// after the first wrong answer rather than before.
pub fn write(profile: &Profile) -> Result<String, FormatError> {
    let mut out = format!("{HEADER} {FORMAT_VERSION}\n");
    for (site, observation) in profile.sites() {
        let ranked = observation.ranked();
        for (witness, _) in &ranked {
            for name in witness.names() {
                if name.chars().any(char::is_whitespace) {
                    return Err(FormatError::UnrepresentableName {
                        name: name.to_owned(),
                    });
                }
            }
        }
        out.push_str(&format!(
            "site {} {} {} {}\n",
            site.module,
            site.position.0,
            observation.total(),
            u8::from(observation.overflowed()),
        ));
        for (witness, count) in ranked {
            out.push_str(&format!("  {count} {witness}\n"));
        }
    }
    Ok(out)
}

/// The profile a record describes.
pub fn read(text: &str) -> Result<Profile, FormatError> {
    let mut lines = text.lines().enumerate().map(|(at, line)| (at + 1, line));

    let (_, header) = lines.next().ok_or(FormatError::NotAProfile)?;
    let mut header = header.split_whitespace();
    if header.next() != Some(HEADER) {
        return Err(FormatError::NotAProfile);
    }
    match header.next().and_then(|v| v.parse::<u32>().ok()) {
        Some(FORMAT_VERSION) => {}
        Some(found) => return Err(FormatError::Version { found }),
        None => return Err(FormatError::NotAProfile),
    }

    let mut profile = Profile::new();
    let mut current: Option<SiteKey> = None;
    for (number, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("site ") {
            current = Some(read_site(number, rest, &mut profile)?);
        } else if let Some(rest) = line.strip_prefix("  ") {
            let site = current.ok_or(FormatError::Malformed {
                line: number,
                expected: "a site line before a witness line",
            })?;
            read_witness(number, rest, site, &mut profile)?;
        } else {
            return Err(FormatError::Malformed {
                line: number,
                expected: "a site line, or a witness line indented by two spaces",
            });
        }
    }
    Ok(profile)
}

/// One site's header, and the arrivals no surviving witness accounts for.
///
/// The unattributed arrivals are recorded by filing them under no witness: a
/// site that overflowed has a total larger than its counts, and reading it back
/// has to preserve that or every share would come back inflated.
fn read_site(number: usize, rest: &str, profile: &mut Profile) -> Result<SiteKey, FormatError> {
    let malformed = FormatError::Malformed {
        line: number,
        expected: "site <module> <position> <total> <overflowed>",
    };
    let mut fields = rest.split_whitespace();
    let module = fields
        .next()
        .and_then(|f| u64::from_str_radix(f, 16).ok())
        .ok_or(malformed.clone())?;
    let position = fields
        .next()
        .and_then(|f| f.parse::<u32>().ok())
        .ok_or(malformed.clone())?;
    let total = fields
        .next()
        .and_then(|f| f.parse::<u64>().ok())
        .ok_or(malformed.clone())?;
    let overflowed = match fields.next() {
        Some("0") => false,
        Some("1") => true,
        _ => return Err(malformed),
    };

    let site = SiteKey::new(ModuleId::from_bits(module), Position(position));
    profile.declare(site, total, overflowed);
    Ok(site)
}

/// One witness line of the site it follows.
fn read_witness(
    number: usize,
    rest: &str,
    site: SiteKey,
    profile: &mut Profile,
) -> Result<(), FormatError> {
    let (count, names) = rest.split_once(' ').unwrap_or((rest, ""));
    let count = count.parse::<u64>().map_err(|_| FormatError::Malformed {
        line: number,
        expected: "a count, then the names",
    })?;
    let witness = Witness::of_all(names.split_whitespace());
    profile.restore(site, &witness, count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::WITNESS_WIDTH;

    fn populated() -> Profile {
        let module = ModuleId::of_source("let p = { x: 1, y: 2 }\n");
        let other = ModuleId::of_source("print(1)\n");
        let mut profile = Profile::new();
        for _ in 0..900 {
            profile.saw(
                SiteKey::new(module, Position(17)),
                &Witness::of_all(["x", "y"]),
            );
        }
        for _ in 0..9 {
            profile.saw(
                SiteKey::new(module, Position(17)),
                &Witness::of_all(["x", "y", "z"]),
            );
        }
        profile.saw(SiteKey::new(other, Position(3)), &Witness::of("callee"));
        profile
    }

    /// A record read back is the profile that was written.
    #[test]
    fn a_record_round_trips() {
        let before = populated();
        let text = write(&before).expect("every name is representable");
        let after = read(&text).expect("this crate's own record");
        assert_eq!(before, after, "reading a record did not answer what wrote it");
    }

    /// An overflowed site comes back overflowed, with its total intact.
    ///
    /// The field most easily lost in a round trip, and losing it inflates every
    /// share on a megamorphic site — which is exactly the site a reader must
    /// decline.
    #[test]
    fn overflow_and_the_unattributed_total_survive_a_round_trip() {
        let module = ModuleId::of_source("x\n");
        let site = SiteKey::new(module, Position(1));
        let mut profile = Profile::new();
        for at in 0..WITNESS_WIDTH + 40 {
            profile.saw(site, &Witness::of(&format!("w{at}")));
        }

        let after = read(&write(&profile).expect("representable")).expect("round trip");
        let observation = after.at(site).expect("the site");
        assert!(observation.overflowed());
        assert_eq!(observation.total(), (WITNESS_WIDTH + 40) as u64);
        assert_eq!(
            observation.ranked().len(),
            WITNESS_WIDTH,
            "more witnesses came back than the record can hold"
        );
    }

    /// The same observations always produce the same bytes.
    #[test]
    fn the_record_is_deterministic() {
        assert_eq!(
            write(&populated()).expect("representable"),
            write(&populated()).expect("representable"),
        );
    }

    /// A record of another version is refused, not interpreted.
    #[test]
    fn a_future_version_is_refused_by_name() {
        let text = format!("{HEADER} 99\n");
        assert_eq!(read(&text), Err(FormatError::Version { found: 99 }));
        assert_eq!(read("something else\n"), Err(FormatError::NotAProfile));
        assert_eq!(read(""), Err(FormatError::NotAProfile));
    }

    /// A witness line with no site before it is refused with its line number.
    #[test]
    fn a_witness_without_a_site_is_refused() {
        let text = format!("{HEADER} {FORMAT_VERSION}\n  3 x y\n");
        assert_eq!(
            read(&text),
            Err(FormatError::Malformed {
                line: 2,
                expected: "a site line before a witness line",
            })
        );
    }

    /// A name that cannot be written is refused at the writer rather than
    /// escaped under the reader.
    #[test]
    fn a_name_with_whitespace_is_refused() {
        let mut profile = Profile::new();
        profile.saw(
            SiteKey::new(ModuleId::of_source("x"), Position(0)),
            &Witness::of("two words"),
        );
        assert_eq!(
            write(&profile),
            Err(FormatError::UnrepresentableName {
                name: "two words".to_owned()
            })
        );
    }
}
