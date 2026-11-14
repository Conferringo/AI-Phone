//! One registry owns shipped Skill installation and observed-application binding.
pub(crate) struct Builtin {
    pub id: &'static str,
    application: Option<&'static str>,
}

pub(crate) const SKILLS: &[Builtin] = &[
    Builtin {
        id: "phone-use",
        application: None,
    },
    Builtin {
        id: "wechat",
        application: Some("com.tencent.mm"),
    },
    Builtin {
        id: "computer-use",
        application: None,
    },
    Builtin {
        id: "device-transfer",
        application: None,
    },
    Builtin {
        id: "autonomous-learning",
        application: None,
    },
    Builtin {
        id: "browser-android",
        application: None,
    },
    Builtin {
        id: "files-android",
        application: None,
    },
    Builtin {
        id: "xiaohongshu",
        application: Some("com.xingin.xhs"),
    },
    Builtin {
        id: "writer",
        application: Some("linux:libreoffice-writer"),
    },
    Builtin {
        id: "calc",
        application: Some("linux:libreoffice-calc"),
    },
    Builtin {
        id: "impress",
        application: Some("linux:libreoffice-impress"),
    },
    Builtin {
        id: "draw",
        application: Some("linux:libreoffice-draw"),
    },
];

pub(crate) fn for_application(application: &str) -> Option<&'static str> {
    SKILLS
        .iter()
        .find(|skill| skill.application == Some(application))
        .map(|skill| skill.id)
}

#[cfg(test)]
pub(crate) fn test_files(root: &std::path::Path) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/builtin");
    for builtin in SKILLS {
        let destination = root.join("skills/builtin").join(builtin.id);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::copy(
            source.join(builtin.id).join("SKILL.md"),
            destination.join("SKILL.md"),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_guides_have_unique_names_and_exact_machine_bindings() {
        let mut names = std::collections::HashSet::new();
        let mut applications = std::collections::HashSet::new();
        for skill in SKILLS {
            assert!(names.insert(skill.id));
            let content = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../skills/builtin")
                    .join(skill.id)
                    .join("SKILL.md"),
            )
            .unwrap();
            assert!(content.contains(&format!("name: {}\n", skill.id)));
            if let Some(application) = skill.application {
                assert!(applications.insert(application));
                assert!(crate::application_identity::valid(application));
            }
        }
        assert_eq!(for_application("com.xingin.xhs"), Some("xiaohongshu"));
        assert_eq!(for_application("linux:libreoffice-writer"), Some("writer"));
        assert_eq!(for_application("linux:libreoffice-calc"), Some("calc"));
        assert_eq!(
            for_application("linux:libreoffice-impress"),
            Some("impress")
        );
        assert_eq!(for_application("linux:libreoffice-draw"), Some("draw"));
        assert_eq!(for_application("com.xingin.xhs.fake"), None);
        assert_eq!(for_application("a private file — LibreOffice Writer"), None);
    }
}
