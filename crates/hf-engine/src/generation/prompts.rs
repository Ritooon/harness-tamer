//! Gabarits de prompts (`resources/prompts/{fr,en}/…`), intégrés au binaire
//! à la compilation. Les variables s'écrivent `{{nom}}`.

use super::sections::SectionId;
use crate::model::Lang;

macro_rules! prompt {
    ($lang:literal, $path:literal) => {
        include_str!(concat!("../../../../resources/prompts/", $lang, "/", $path))
    };
}

/// Identifiant d'un gabarit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    AgentsSystem,
    AgentsUser,
    Section(SectionId),
    SkillsSystem,
    SkillsCreate,
    SkillsAdapt,
}

pub fn template(t: Template, lang: Lang) -> &'static str {
    use SectionId as S;
    match lang {
        Lang::Fr => match t {
            Template::AgentsSystem => prompt!("fr", "agents/system.md"),
            Template::AgentsUser => prompt!("fr", "agents/user.md"),
            Template::Section(S::Context) => prompt!("fr", "agents/context.md"),
            Template::Section(S::Goals) => prompt!("fr", "agents/goals.md"),
            Template::Section(S::Stack) => prompt!("fr", "agents/stack.md"),
            Template::Section(S::Structure) => prompt!("fr", "agents/structure.md"),
            Template::Section(S::Commands) => prompt!("fr", "agents/commands.md"),
            Template::Section(S::Conventions) => prompt!("fr", "agents/conventions.md"),
            Template::Section(S::Quality) => prompt!("fr", "agents/quality.md"),
            Template::Section(S::Prohibitions) => prompt!("fr", "agents/prohibitions.md"),
            Template::Section(S::Git) => prompt!("fr", "agents/git.md"),
            Template::Section(S::Ui) => prompt!("fr", "agents/ui.md"),
            Template::SkillsSystem => prompt!("fr", "skills/system.md"),
            Template::SkillsCreate => prompt!("fr", "skills/create.md"),
            Template::SkillsAdapt => prompt!("fr", "skills/adapt.md"),
        },
        Lang::En => match t {
            Template::AgentsSystem => prompt!("en", "agents/system.md"),
            Template::AgentsUser => prompt!("en", "agents/user.md"),
            Template::Section(S::Context) => prompt!("en", "agents/context.md"),
            Template::Section(S::Goals) => prompt!("en", "agents/goals.md"),
            Template::Section(S::Stack) => prompt!("en", "agents/stack.md"),
            Template::Section(S::Structure) => prompt!("en", "agents/structure.md"),
            Template::Section(S::Commands) => prompt!("en", "agents/commands.md"),
            Template::Section(S::Conventions) => prompt!("en", "agents/conventions.md"),
            Template::Section(S::Quality) => prompt!("en", "agents/quality.md"),
            Template::Section(S::Prohibitions) => prompt!("en", "agents/prohibitions.md"),
            Template::Section(S::Git) => prompt!("en", "agents/git.md"),
            Template::Section(S::Ui) => prompt!("en", "agents/ui.md"),
            Template::SkillsSystem => prompt!("en", "skills/system.md"),
            Template::SkillsCreate => prompt!("en", "skills/create.md"),
            Template::SkillsAdapt => prompt!("en", "skills/adapt.md"),
        },
    }
}

/// Remplace `{{clé}}` par sa valeur. Les valeurs ne sont pas réinterprétées :
/// un `{{x}}` présent dans le texte de l'utilisateur reste tel quel.
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() * 2);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let key = after[..end].trim();
                match vars.iter().find(|(k, _)| *k == key) {
                    Some((_, value)) => out.push_str(value),
                    None => out.push_str(&rest[start..start + 2 + end + 2]),
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_exists_in_both_languages() {
        for lang in [Lang::Fr, Lang::En] {
            for t in [Template::AgentsSystem, Template::AgentsUser, Template::SkillsSystem, Template::SkillsCreate]
                .into_iter()
                .chain(SectionId::ALL.map(Template::Section))
            {
                assert!(template(t, lang).len() > 20, "{t:?} {lang:?}");
            }
        }
        assert!(template(Template::AgentsUser, Lang::Fr).contains("{{task}}"));
    }

    #[test]
    fn rendering_does_not_reinterpret_values() {
        let out = render("A {{x}} B {{ y }} C {{inconnu}} {{", &[("x", "{{y}}"), ("y", "2")]);
        assert_eq!(out, "A {{y}} B 2 C {{inconnu}} {{");
    }
}
