//! Where PeDeeFe lives on the web. One table, so the Help menu, the About dialog, the home
//! screen, the CLI and the README agree.
//!
//! PeDeeFe is a fork of PrintCraft by the ArtCraft team. The fork doesn't use the ArtCraft name
//! or marks and doesn't link to ArtCraft's community (docs: `NOTICE`, "Forks and modified
//! versions" in PrintCraft's brand licence); it says, in plain text, what it is based on.

/// The app's name, as people see it (window title, About, installer).
pub const APP_NAME: &str = "PeDeeFe";

pub const GITHUB: &str = "https://github.com/Phreezi/printcraft";
pub const ISSUES: &str = "https://github.com/Phreezi/printcraft/issues";
/// Where this fork's builds are published (Help ▸ Check for updates).
pub const RELEASES: &str = "https://github.com/Phreezi/printcraft/releases";
/// The project PeDeeFe is based on.
pub const UPSTREAM: &str = "https://github.com/storytold/printcraft";

/// A link and the registry command that opens it.
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub command: &'static str,
    pub label: &'static str,
    pub url: &'static str,
    /// Lucide icon name.
    pub icon: &'static str,
}

/// In the order they are shown.
pub const LINKS: &[Link] = &[
    Link { command: "help.issues", label: "Report a problem", url: ISSUES, icon: "message-square-text" },
    Link { command: "help.github", label: "PeDeeFe on GitHub", url: GITHUB, icon: "code-xml" },
    Link { command: "help.upstream", label: "Based on PrintCraft", url: UPSTREAM, icon: "book-open" },
];

pub fn for_command(id: &str) -> Option<&'static Link> {
    LINKS.iter().find(|l| l.command == id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_are_this_forks_and_registered() {
        assert!(super::ISSUES.starts_with(super::GITHUB));
        assert!(super::RELEASES.starts_with(super::GITHUB));
        for l in super::LINKS {
            assert!(l.url.starts_with("https://github.com/"), "{}", l.url);
            assert!(!l.label.contains("ArtCraft") && !l.url.contains("artcraft"), "{}", l.url);
            assert!(crate::commands::command(l.command).is_some(), "{} is a registered command", l.command);
        }
    }
}
