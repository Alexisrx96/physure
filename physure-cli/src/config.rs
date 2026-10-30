use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PhysureConfig {
    pub lang: String,
}

pub struct I18nLabels {
    pub abstract_title: &'static str,
    pub sec_evaluations: &'static str,
    pub sec_appendix: &'static str,
    pub fig_prefix: &'static str,
    pub footer_engine: &'static str,
    pub html_lang: &'static str,
    pub solve_from: &'static str,
    pub solve_solving_for: &'static str,
    pub given_prefix: &'static str,
    pub if_word: &'static str,
    pub then_word: &'static str,
    pub else_word: &'static str,
    pub true_word: &'static str,
    pub false_word: &'static str,
    pub manuscript_title: &'static str,
    pub html_generated: &'static str,
}

impl PhysureConfig {
    pub fn load() -> Self {
        let mut lang = "en".to_string();

        let mut candidate_paths = vec![
            PathBuf::from("physure.conf"),
            PathBuf::from("../physure.conf"),
        ];

        if let Ok(user_dir) = env::var("USERPROFILE").or_else(|_| env::var("HOME")) {
            candidate_paths.push(PathBuf::from(&user_dir).join(".config/physure/physure.conf"));
            candidate_paths.push(PathBuf::from(&user_dir).join("physure.conf"));
            candidate_paths.push(PathBuf::from(&user_dir).join(".physure.conf"));
        }

        for path in candidate_paths {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with('#') || trimmed.starts_with(';') || trimmed.is_empty() {
                            continue;
                        }
                        if let Some((k, v)) = trimmed.split_once('=') {
                            let key = k.trim().to_lowercase();
                            let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
                            if key == "lang" || key == "language" {
                                lang = val.to_lowercase();
                            }
                        }
                    }
                }
                break;
            }
        }

        PhysureConfig { lang }
    }

    pub fn i18n(&self) -> I18nLabels {
        match self.lang.as_str() {
            "es" | "es-es" | "es-mx" => I18nLabels {
                abstract_title: "Resumen / Abstract",
                sec_evaluations: "1. Evaluaciones Físicas y Ecuaciones",
                sec_appendix: "2. Apéndice A: Código Fuente PHS",
                fig_prefix: "Figura",
                footer_engine: "Motor Physure Core",
                html_lang: "es",
                solve_from: "De",
                solve_solving_for: "despejando",
                given_prefix: "Dado",
                if_word: "si",
                then_word: "entonces",
                else_word: "si no",
                true_word: "Verdadero",
                false_word: "Falso",
                manuscript_title: "Manuscrito Científico Physure",
                html_generated: "Manuscrito científico HTML generado (100% offline):",
            },
            "fr" => I18nLabels {
                abstract_title: "Résumé / Abstract",
                sec_evaluations: "1. Évaluations Physiques et Équations",
                sec_appendix: "2. Annexe A : Code Source PHS",
                fig_prefix: "Figure",
                footer_engine: "Moteur Physure Core",
                html_lang: "fr",
                solve_from: "À partir de",
                solve_solving_for: "en résolvant pour",
                given_prefix: "Étant donné",
                if_word: "si",
                then_word: "alors",
                else_word: "sinon",
                true_word: "Vrai",
                false_word: "Faux",
                manuscript_title: "Manuscrit scientifique Physure",
                html_generated: "Manuscrit scientifique HTML généré (100 % hors ligne) :",
            },
            "de" => I18nLabels {
                abstract_title: "Zusammenfassung / Abstract",
                sec_evaluations: "1. Physikalische Auswertungen und Gleichungen",
                sec_appendix: "2. Anhang A: PHS-Quellcode",
                fig_prefix: "Abbildung",
                footer_engine: "Physure Core Engine",
                html_lang: "de",
                solve_from: "Aus",
                solve_solving_for: "aufgelöst nach",
                given_prefix: "Gegeben",
                if_word: "wenn",
                then_word: "dann",
                else_word: "sonst",
                true_word: "Wahr",
                false_word: "Falsch",
                manuscript_title: "Wissenschaftliches Manuskript Physure",
                html_generated: "Wissenschaftliches HTML-Manuskript erstellt (100 % offline):",
            },
            _ => I18nLabels {
                abstract_title: "Abstract",
                sec_evaluations: "1. Physical Evaluations and Equations",
                sec_appendix: "2. Appendix A: PHS Source Code",
                fig_prefix: "Figure",
                footer_engine: "Physure Core Engine",
                html_lang: "en",
                solve_from: "From",
                solve_solving_for: "solving for",
                given_prefix: "Given",
                if_word: "if",
                then_word: "then",
                else_word: "else",
                true_word: "True",
                false_word: "False",
                manuscript_title: "Physure Scientific Manuscript",
                html_generated: "Scientific HTML manuscript generated (100% offline):",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(lang: &str) -> I18nLabels {
        PhysureConfig { lang: lang.to_string() }.i18n()
    }

    #[test]
    fn english_is_the_default_and_an_unknown_language_falls_back_to_it() {
        for lang in ["en", "", "pt", "xx"] {
            let l = labels(lang);
            assert_eq!(l.html_lang, "en", "{lang}");
            assert_eq!(l.manuscript_title, "Physure Scientific Manuscript", "{lang}");
            assert!(l.html_generated.starts_with("Scientific HTML manuscript generated"), "{lang}");
        }
    }

    #[test]
    fn every_language_has_its_own_manuscript_title_and_message() {
        let en = labels("en");
        for (lang, title) in [
            ("es", "Manuscrito Científico Physure"),
            ("es-mx", "Manuscrito Científico Physure"),
            ("fr", "Manuscrit scientifique Physure"),
            ("de", "Wissenschaftliches Manuskript Physure"),
        ] {
            let l = labels(lang);
            assert_eq!(l.manuscript_title, title, "{lang}");
            assert_ne!(l.html_generated, en.html_generated, "{lang}");
            assert!(!l.html_generated.is_empty(), "{lang}");
        }
    }
}
