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
            },
        }
    }
}
