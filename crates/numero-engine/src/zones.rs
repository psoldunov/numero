//! Time zones by abbreviation (`PST`, `HKT`) and by city (`Berlin`, `New
//! York`), offline. There are no fixed offsets: Numi reads `GMT+3` as the
//! zone `GMT` followed by `+ 3` (§8.2 [oracle]). Abbreviations name the
//! region's zone, so `PST` means Los Angeles time and follows its daylight
//! saving, the way macOS's `TimeZone(abbreviation:)` resolves them.

use chrono_tz::{TZ_VARIANTS, Tz};

use crate::trie::Trie;
use crate::vocab::Meaning;

const CITIES: &str = include_str!("../data/cities.tsv");

/// Abbreviation → IANA zone: macOS's `TimeZone.abbreviationDictionary`
/// (§8.3), then a few common ones it lacks. Each also matches lower-cased.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("ADT", "America/Halifax"),
    ("AKDT", "America/Juneau"),
    ("AKST", "America/Juneau"),
    ("ART", "America/Argentina/Buenos_Aires"),
    ("AST", "America/Halifax"),
    ("BDT", "Asia/Dhaka"),
    ("BRST", "America/Sao_Paulo"),
    ("BRT", "America/Sao_Paulo"),
    ("BST", "Europe/London"),
    ("CAT", "Africa/Harare"),
    ("CDT", "America/Chicago"),
    ("CEST", "Europe/Paris"),
    ("CET", "Europe/Paris"),
    ("CLST", "America/Santiago"),
    ("CLT", "America/Santiago"),
    ("COT", "America/Bogota"),
    ("CST", "America/Chicago"),
    ("EAT", "Africa/Addis_Ababa"),
    ("EDT", "America/New_York"),
    ("EEST", "Europe/Athens"),
    ("EET", "Europe/Athens"),
    ("EST", "America/New_York"),
    ("GMT", "Etc/GMT"),
    ("GST", "Asia/Dubai"),
    ("HKT", "Asia/Hong_Kong"),
    ("HST", "Pacific/Honolulu"),
    ("ICT", "Asia/Bangkok"),
    ("IRST", "Asia/Tehran"),
    ("IST", "Asia/Kolkata"),
    ("JST", "Asia/Tokyo"),
    ("KST", "Asia/Seoul"),
    ("MDT", "America/Denver"),
    ("MSD", "Europe/Moscow"),
    ("MSK", "Europe/Moscow"),
    ("MST", "America/Phoenix"),
    ("NZDT", "Pacific/Auckland"),
    ("NZST", "Pacific/Auckland"),
    ("PDT", "America/Los_Angeles"),
    ("PET", "America/Lima"),
    ("PHT", "Asia/Manila"),
    ("PKT", "Asia/Karachi"),
    ("PST", "America/Los_Angeles"),
    ("SGT", "Asia/Singapore"),
    ("TRT", "Europe/Istanbul"),
    ("UTC", "UTC"),
    ("WAT", "Africa/Lagos"),
    ("WEST", "Europe/Lisbon"),
    ("WET", "Europe/Lisbon"),
    ("WIT", "Asia/Jakarta"),
    // Not in the macOS table.
    ("SAST", "Africa/Johannesburg"),
    ("AEST", "Australia/Sydney"),
    ("AEDT", "Australia/Sydney"),
    ("ACST", "Australia/Adelaide"),
    ("AWST", "Australia/Perth"),
    ("NST", "America/St_Johns"),
];

/// IANA region prefixes whose last component names a city.
const CITY_REGIONS: &[&str] = &[
    "Africa/",
    "America/",
    "Asia/",
    "Atlantic/",
    "Australia/",
    "Europe/",
    "Indian/",
    "Pacific/",
];

/// IANA components that are not cities.
const NOT_CITIES: &[&str] = &[
    "North", "South", "West", "East", "Central", "Yukon", "Canberra",
];

pub fn add_zone_phrases(trie: &mut Trie) {
    for (abbreviation, name) in ABBREVIATIONS {
        if let Ok(tz) = name.parse::<Tz>() {
            trie.insert(abbreviation, true, Meaning::Zone(tz));
            trie.insert(&abbreviation.to_lowercase(), true, Meaning::Zone(tz));
        }
    }
    for line in CITIES
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let Some((city, zone)) = line.split_once('\t') else {
            continue;
        };
        if let Ok(tz) = zone.trim().parse::<Tz>() {
            let city = city.trim();
            trie.insert(
                city,
                city.chars().count() <= 3 && city.chars().all(char::is_uppercase),
                Meaning::Zone(tz),
            );
        }
    }
    for tz in TZ_VARIANTS {
        let name = tz.name();
        if !CITY_REGIONS.iter().any(|region| name.starts_with(region)) {
            continue;
        }
        let Some(city) = name.rsplit('/').next() else {
            continue;
        };
        if city.chars().all(|c| c.is_uppercase() || c == '_') || NOT_CITIES.contains(&city) {
            continue;
        }
        trie.insert(&city.replace('_', " "), false, Meaning::Zone(tz));
    }
}

/// Whether chrono-tz knows an IANA zone name.
#[cfg(test)]
fn is_known(name: &str) -> bool {
    name.parse::<Tz>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_abbreviation_resolves() {
        for (abbreviation, name) in ABBREVIATIONS {
            assert!(is_known(name), "{abbreviation} → {name}");
        }
    }

    #[test]
    fn every_city_zone_resolves() {
        for line in CITIES
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (city, zone) = line.split_once('\t').expect("tab-separated");
            assert!(is_known(zone.trim()), "{city} → {zone}");
        }
    }
}
