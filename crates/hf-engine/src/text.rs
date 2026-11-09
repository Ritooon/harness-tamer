//! Petits utilitaires de texte : repli des accents, slugs, troncature.

/// Minuscules sans accents, pour une recherche tolérante (« Éditeur » ≈ « editeur »).
pub fn fold(input: &str) -> String {
    input
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            'œ' => 'o',
            'æ' => 'a',
            other => other,
        })
        .collect()
}

/// Identifiant en kebab-case (`Revue de code React` → `revue-de-code-react`),
/// limité à 64 caractères comme l'exige le format SKILL.md.
pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for c in fold(input).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.len() > 64 {
        slug.truncate(64);
        while slug.ends_with('-') {
            slug.pop();
        }
    }
    slug
}

/// Coupe un texte à `max` caractères (frontière de caractère), avec « … ».
pub fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_owned();
    }
    let mut out: String = input.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_removes_accents_and_case() {
        assert_eq!(fold("Éditeur Évolué ÇA"), "editeur evolue ca");
    }

    #[test]
    fn slugs_are_kebab_case_and_bounded() {
        assert_eq!(slugify("  Revue de code — React !"), "revue-de-code-react");
        assert_eq!(slugify(&"a".repeat(80)).len(), 64);
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        assert_eq!(truncate_chars("ééééé", 3), "éé…");
        assert_eq!(truncate_chars("abc", 3), "abc");
    }
}
