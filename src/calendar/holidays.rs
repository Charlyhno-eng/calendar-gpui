use chrono::{Datelike, Duration, NaiveDate};

/// French national public holidays; excludes regional and overseas additions.
/// See https://www.service-public.gouv.fr/particuliers/vosdroits/F2405 .
pub fn holidays(date: NaiveDate) -> Vec<&'static str> {
    let fixed = match (date.month(), date.day()) {
        (1, 1) => Some("New Year's Day"),
        (5, 1) => Some("Labour Day"),
        (5, 8) => Some("Victory Day"),
        (7, 14) => Some("Bastille Day"),
        (8, 15) => Some("Assumption"),
        (11, 1) => Some("All Saints' Day"),
        (11, 11) => Some("Armistice Day"),
        (12, 25) => Some("Christmas Day"),
        _ => None,
    };
    let mut names: Vec<_> = fixed.into_iter().collect();
    let easter = easter_sunday(date.year());
    for (offset, name) in [
        (1, "Easter Monday"),
        (39, "Ascension Day"),
        (50, "Whit Monday"),
    ] {
        if date == easter + Duration::days(offset) {
            names.push(name);
        }
    }
    names
}

/// Gregorian computus (Meeus/Jones/Butcher), valid for the supported years.
fn easter_sunday(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let n = h + l - 7 * m + 114;
    NaiveDate::from_ymd_opt(year, (n / 31) as u32, (n % 31 + 1) as u32)
        .expect("Gregorian Easter is a valid date")
}
