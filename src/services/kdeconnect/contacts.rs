/*
 * the phone's contacts, as kdeconnect's contacts plugin leaves them: one
 * vcard per contact under ~/.local/share/kpeoplevcard/kdeconnect-<device>/,
 * read for names to put on numbers
 */

use std::fs;
use std::path::{Path, PathBuf};

use super::files;

// shorter numbers (service codes) only match whole
const SUFFIX_DIGITS: usize = 7;

// one person in the phone's contacts
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Person {
    pub(crate) name: String,

    // their numbers as the card writes them
    pub(crate) numbers: Vec<String>,

    // the card's photo, base64 as it came, until it's written out
    photo_data: Option<String>,

    // the photo, written out
    pub(crate) photo: Option<PathBuf>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Contacts {
    // each number's digits and whose it is, by index into people
    numbers: Vec<(String, usize)>,

    people: Vec<Person>,
}

impl Contacts {
    pub(crate) fn load(device: &str) -> Self {
        let Some(folder) = folder(device) else {
            return Self::default();
        };

        let Ok(entries) = fs::read_dir(folder) else {
            return Self::default();
        };

        let mut contacts = Self::default();

        for entry in entries.flatten() {
            let path = entry.path();

            if path.extension().is_none_or(|extension| extension != "vcf") {
                continue;
            }

            if let Ok(text) = fs::read_to_string(&path) {
                contacts.add_vcard(&text);
            }
        }

        contacts.write_photos();

        contacts
    }

    pub(crate) fn add_vcard(&mut self, text: &str) {
        let mut person = Person::default();

        for line in unfold(text) {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };

            // TEL;TYPE=CELL:+47 ... and item1.TEL:... both name a number
            let mut parameters = key.split(';');

            let property = parameters
                .next()
                .unwrap_or("")
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();

            match property.as_str() {
                "FN" => person.name = unescape(value),
                "TEL" => {
                    let number = value.trim_start_matches("tel:").trim();

                    if !digits(number).is_empty() {
                        person.numbers.push(number.to_string());
                    }
                }
                // PHOTO;ENCODING=BASE64;JPEG:... (2.1), ENCODING=b (3.0), or a data: url (4.0)
                "PHOTO" => {
                    let inline = parameters
                        .any(|parameter| parameter.to_ascii_uppercase().contains("ENCODING=B"));

                    let data = match value.split_once("base64,") {
                        Some((_, data)) => Some(data),
                        None if inline => Some(value),
                        None => None,
                    };

                    if let Some(data) = data.filter(|data| !data.trim().is_empty()) {
                        person.photo_data = Some(data.trim().to_string());
                    }
                }
                _ => {}
            }
        }

        if person.name.is_empty() {
            return;
        }

        let index = self.people.len();

        for number in &person.numbers {
            self.numbers.push((digits(number), index));
        }

        self.people.push(person);
    }

    // each photo decoded into the cache once, named for what it holds
    fn write_photos(&mut self) {
        for person in &mut self.people {
            let Some(data) = person.photo_data.take() else {
                continue;
            };

            let name = format!("{}.jpg", files::name_for(&data));

            person.photo = files::write_once("contacts", &name, &data);
        }
    }

    fn person(&self, address: &str) -> Option<&Person> {
        let wanted = digits(address);

        if wanted.is_empty() {
            return None;
        }

        self.numbers
            .iter()
            .find(|(number, _)| same_number(number, &wanted))
            .map(|(_, index)| &self.people[*index])
    }

    /*
     * the name for `address`: the same digits, or one ending the other, so
     * a local number and its form with the country code are one contact
     */
    pub(crate) fn name(&self, address: &str) -> Option<&str> {
        self.person(address).map(|person| person.name.as_str())
    }

    pub(crate) fn photo(&self, address: &str) -> Option<&Path> {
        self.person(address)?.photo.as_deref()
    }

    pub(crate) fn people(&self) -> &[Person] {
        &self.people
    }
}

fn folder(device: &str) -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;

    Some(
        data.join("kpeoplevcard")
            .join(format!("kdeconnect-{device}")),
    )
}

pub(crate) fn same_number(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }

    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };

    short.len() >= SUFFIX_DIGITS && long.ends_with(short)
}

pub(crate) fn digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

// a line folded onto the next starts with a space or a tab (rfc 6350)
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim_end_matches('\r');

        match (line.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_string()),
        }
    }

    lines
}

fn unescape(value: &str) -> String {
    value
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\n", " ")
        .replace("\\\\", "\\")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Ola Nord\r\n mann\r\nTEL;CELL:+47 957 23 015\r\nitem1.TEL:22 33 44 55\r\nEND:VCARD\r\n";

    #[test]
    fn reads_names_and_numbers_from_folded_lines() {
        let mut contacts = Contacts::default();

        contacts.add_vcard(CARD);

        assert_eq!(contacts.name("+4795723015"), Some("Ola Nordmann"));
        assert_eq!(contacts.name("22334455"), Some("Ola Nordmann"));
    }

    #[test]
    fn a_local_number_matches_its_international_form() {
        let mut contacts = Contacts::default();

        contacts.add_vcard(CARD);

        assert_eq!(contacts.name("95723015"), Some("Ola Nordmann"));
        assert_eq!(contacts.name("(957) 23-015"), Some("Ola Nordmann"));
    }

    #[test]
    fn short_numbers_only_match_whole() {
        let mut contacts = Contacts::default();

        contacts.add_vcard("BEGIN:VCARD\nFN:Bank\nTEL:2222\nEND:VCARD\n");

        assert_eq!(contacts.name("2222"), Some("Bank"));
        assert_eq!(contacts.name("12222"), None);
        assert_eq!(contacts.name("015"), None);
        assert_eq!(contacts.name(""), None);
    }

    #[test]
    fn reads_a_folded_photo() {
        let mut contacts = Contacts::default();

        contacts.add_vcard(
            "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Kari\r\nTEL:95723015\r\nPHOTO;ENCODING=BASE64;JPEG:/9j/4AAQ\r\n SkZJRg\r\nEND:VCARD\r\n",
        );
        contacts.add_vcard(
            "BEGIN:VCARD\nVERSION:4.0\nFN:Ola\nTEL:22334455\nPHOTO:data:image/jpeg;base64,aGVp\nEND:VCARD\n",
        );

        assert_eq!(
            contacts.people()[0].photo_data.as_deref(),
            Some("/9j/4AAQSkZJRg")
        );
        assert_eq!(contacts.people()[1].photo_data.as_deref(), Some("aGVp"));
        assert_eq!(contacts.people()[0].numbers, ["95723015"]);
    }

    #[test]
    fn a_card_without_a_name_adds_nothing() {
        let mut contacts = Contacts::default();

        contacts.add_vcard("BEGIN:VCARD\nTEL:+4795723015\nEND:VCARD\n");

        assert_eq!(contacts.name("95723015"), None);
    }
}
