//! The text the language server shows in more than one language.
//!
//! English is the source text and the default. Every other language lives in this file and
//! nowhere else in the crate; the client picks one with the `locale` it sends in `initialize`
//! (a BCP 47 tag such as `es` or `es-MX`), and a language without a table here falls back to
//! English.

/// Whether `locale` asks for Spanish (`es`, `es-MX`, `es_ES`, ...).
fn is_spanish(locale: &str) -> bool {
    let tag = locale.trim().to_ascii_lowercase();
    tag == "es" || tag.starts_with("es-") || tag.starts_with("es_")
}

/// What a unit measures, in the language of `locale`.
pub fn quantity_name(unit: &str, locale: &str) -> Option<&'static str> {
    let es = is_spanish(locale);
    Some(match unit {
        "m" => if es { "Longitud" } else { "Length" },
        "kg" => if es { "Masa" } else { "Mass" },
        "s" => if es { "Tiempo" } else { "Time" },
        "N" => if es { "Fuerza" } else { "Force" },
        "Pa" => if es { "Presión / Esfuerzo" } else { "Pressure / Stress" },
        "J" => if es { "Energía / Trabajo" } else { "Energy / Work" },
        "W" => if es { "Potencia" } else { "Power" },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default() {
        for locale in ["", "en", "en-US", "fr", "xx"] {
            assert_eq!(quantity_name("m", locale), Some("Length"), "{locale:?}");
            assert_eq!(quantity_name("W", locale), Some("Power"), "{locale:?}");
        }
    }

    #[test]
    fn spanish_is_chosen_by_the_locale_the_client_sends() {
        for locale in ["es", "ES", "es-MX", "es_ES", " es "] {
            assert_eq!(quantity_name("m", locale), Some("Longitud"), "{locale:?}");
            assert_eq!(quantity_name("kg", locale), Some("Masa"), "{locale:?}");
            assert_eq!(quantity_name("Pa", locale), Some("Presión / Esfuerzo"), "{locale:?}");
        }
        // a language that merely starts with the letters is not Spanish
        assert_eq!(quantity_name("m", "esperanto"), Some("Length"));
    }

    #[test]
    fn a_unit_without_a_name_has_none_in_any_language() {
        assert_eq!(quantity_name("furlong", "en"), None);
        assert_eq!(quantity_name("furlong", "es"), None);
    }
}
